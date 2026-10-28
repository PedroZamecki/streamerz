//! A popover: a small panel over the app's window, near what it's about, that closes with a click
//! outside it or Escape. Not a window of its own (those can't be placed on Wayland, and tiling desktops
//! tile them). It's a window inside the window, so widgets repainting themselves under it (a cursor, a
//! hover) never draw over it.

use fltk::enums::{Color, Event, FrameType, Key};
use fltk::frame::Frame;
use fltk::prelude::*;
use fltk::window::Window;
use fltk::{app, draw};
use std::cell::Cell;
use std::rc::Rc;

pub struct Popover {
    host: Box<dyn WindowExt>,
    pop: Window,
    catcher: Frame,
    /// Set by a click outside, Escape, or `close_now`.
    pub done: Rc<Cell<bool>>,
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
        let mut pop = Window::new(x, y, w, h, None);
        pop.set_frame(FrameType::FlatBox);
        pop.set_color(Color::Background);
        pop.draw(|c| {
            draw::set_draw_color(Color::color_average(Color::Foreground, Color::Background, 0.25));
            draw::draw_rounded_rect(0, 0, c.w(), c.h(), 8);
        });
        Popover {
            host,
            pop,
            catcher,
            done,
        }
    }

    /// The popover's size.
    pub fn size(&self) -> (i32, i32) {
        (self.pop.w(), self.pop.h())
    }

    /// The widgets are all in.
    pub fn finish(&mut self) {
        self.pop.end();
        self.host.end();
        self.host.redraw();
    }

    /// Waits until it's closed, calling `tick` a few times a second (and on every event).
    pub fn run(&self, mut tick: impl FnMut()) {
        while !self.done.get() {
            let _ = app::wait_for(0.2);
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
