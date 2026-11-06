//! A popover: a small panel over the app's window, near what it's about, that closes with a click
//! outside it or Escape. Not a window of its own (those can't be placed on Wayland, and tiling desktops
//! tile them). It's a window inside the window, so widgets repainting themselves under it (a cursor, a
//! hover) never draw over it.

use fltk::enums::{Color, Damage, Event, FrameType, Key};
use fltk::frame::Frame;
use fltk::image::RgbImage;
use fltk::prelude::*;
use fltk::surface::ImageSurface;
use fltk::window::Window;
use fltk::{app, draw};
use std::cell::Cell;
use std::rc::Rc;

pub struct Popover {
    /// Never dropped: a window got from `first_window()` owns its widget, and dropping it would delete the
    /// app's window.
    host: std::mem::ManuallyDrop<Box<dyn WindowExt>>,
    pop: Window,
    catcher: Frame,
    /// Set by a click outside, Escape, or `close_now`.
    pub done: Rc<Cell<bool>>,
    /// Its full height, while it waits to unroll to it (it opens shorter when animations are on).
    full: Cell<Option<i32>>,
}

/// The corner radius of everything that floats (popovers, menus).
pub const RADIUS: i32 = 8;

/// The part of `host` at `(x, y, w, h)`, as it is drawn now.
pub(super) fn backdrop(host: &dyn WindowExt, x: i32, y: i32, w: i32, h: i32) -> Option<RgbImage> {
    let surface = ImageSurface::new(w, h, false);
    ImageSurface::push_current(&surface);
    draw::set_draw_color(Color::Background);
    draw::draw_rectf(0, 0, w, h);
    // Not dropped: a handle got this way could take the window with it.
    if let Some(base) = fltk::widget::Widget::from_dyn_widget_ptr(host.as_widget_ptr()).map(std::mem::ManuallyDrop::new)
    {
        surface.draw(&*base, -x, -y);
    }
    ImageSurface::pop_current();
    surface.image()
}

/// A floating panel `w` x `h` at the window's corner: what it covers in its corners, then a rounded
/// panel with a quiet border.
fn panel(w: i32, h: i32, under: Option<&RgbImage>) {
    let back = Color::Background;
    match under {
        Some(img) => img.clone().draw(0, 0, w, h),
        None => draw::draw_rect_fill(0, 0, w, h, back),
    }
    draw::set_draw_color(back);
    draw::draw_rounded_rectf(0, 0, w, h, RADIUS);
    draw::set_draw_color(Color::color_average(Color::Foreground, back, 0.2));
    draw::draw_rounded_rect(0, 0, w, h, RADIUS);
}

impl Popover {
    /// A `w` x `h` popover in `host` with its corner at `(x, y)`, moved to fit. Build its widgets next,
    /// then `finish`.
    pub fn start(host: Box<dyn WindowExt>, x: i32, y: i32, w: i32, h: i32) -> Popover {
        let (w, h) = (w.min(host.w() - 16), h.min(host.h() - 16));
        let (x, y) = (
            x.clamp(8, (host.w() - w - 8).max(8)),
            y.clamp(8, (host.h() - h - 8).max(8)),
        );
        let done = Rc::new(Cell::new(false));
        host.begin();
        // Under the popover and over everything else: it takes every click outside, and closes the popover.
        let mut catcher = Frame::new(0, 0, host.w(), host.h(), None);
        catcher.handle({
            let done = done.clone();
            move |_, ev| match ev {
                Event::Push => {
                    done.set(true);
                    true
                }
                Event::Released | Event::Enter | Event::Move | Event::MouseWheel => true,
                Event::KeyDown | Event::Shortcut if app::event_key() == Key::Escape => {
                    done.set(true);
                    true
                }
                _ => false,
            }
        });
        // What is under it, for its corners: a window is square, so they show what they cover.
        let under = backdrop(&*host, x, y, w, h);
        let mut pop = Window::new(x, y, w, h, None);
        pop.set_frame(FrameType::NoBox);
        pop.set_color(Color::Background);
        pop.super_draw_first(false);
        pop.draw(move |c| {
            // Only a child to repaint: it paints itself (as FLTK's group does, which then skips the box).
            if c.damage_type() == Damage::Child {
                return;
            }
            panel(c.w(), c.h(), under.as_ref());
        });
        Popover {
            host: std::mem::ManuallyDrop::new(host),
            pop,
            catcher,
            done,
            full: Cell::new(None),
        }
    }

    /// The popover's window, to repaint it all after its widgets moved (no widget clears the old pixels).
    pub fn window(&self) -> Window {
        self.pop.clone()
    }

    /// The popover's size.
    pub fn size(&self) -> (i32, i32) {
        (self.pop.w(), self.pop.h())
    }

    /// The widgets are all in: it opens (shorter, when animations are on: `run` unrolls it, once what
    /// takes the keyboard has it, so keys pressed meanwhile go there).
    pub fn finish(&mut self) {
        self.pop.end();
        self.host.end();
        if super::anim::on() {
            // Its widgets stay where they are while it grows: it uncovers them.
            let (x, y, w, h) = (self.pop.x(), self.pop.y(), self.pop.w(), self.pop.h());
            self.full.set(Some(h));
            self.pop.make_resizable(false);
            self.pop.resize(x, y, w, h * 3 / 5);
        }
        // A window made after its parent was shown has to be shown itself.
        self.pop.show();
        self.host.redraw();
    }

    /// Unrolls it down to its full height (`run` does it; a popover waiting in a loop of its own calls it
    /// once its keyboard focus is set).
    pub fn unroll(&self) {
        let Some(h) = self.full.take() else { return };
        let mut pop = self.pop.clone();
        let (x, y, w) = (pop.x(), pop.y(), pop.w());
        super::anim::play(super::anim::POP, |p| {
            if !pop.was_deleted() {
                pop.resize(x, y, w, super::anim::mix(h * 3 / 5, h, p));
                pop.redraw();
            }
        });
        if !pop.was_deleted() {
            pop.make_resizable(true);
            pop.init_sizes();
        }
    }

    /// Waits until it's closed, calling `tick` a few times a second (and on every event).
    pub fn run(&self, mut tick: impl FnMut()) {
        self.unroll();
        while !self.done.get() {
            super::nested_wait(Some(0.2));
            tick();
        }
    }

    /// Takes it away.
    pub fn close(mut self) {
        app::delete_widget(self.catcher);
        app::delete_widget(self.pop);
        self.host.redraw();
    }
}
