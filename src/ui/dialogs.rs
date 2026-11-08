//! Modal dialogs: JSON editor, type picker, the action form, import and export.

use super::fit::Ellipsis;
use super::popover::RADIUS;
use super::{button, ROW};
use crate::i18n::{tr, trf};
use crate::model::{Action, Obj};
use fltk::browser::CheckBrowser;
use fltk::button::CheckButton;
use fltk::enums::{Align, Color, Damage, Event, Font, FrameType, Key, Shortcut};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::image::RgbImage;
use fltk::input::{Input, MultilineInput};
use fltk::misc::InputChoice;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextEditor};
use fltk::window::{DoubleWindow, Window};
use fltk::{app, dialog, draw};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The dialog's answer: None while open, then OK (true) or cancel (false).
pub type Answer = Rc<Cell<Option<bool>>>;

/// What a dialog is about, for its colour: a plain one, or news, a question, a success, a warning or
/// an error (its border and its icon take the colour).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Plain,
    Info,
    Question,
    Success,
    Warning,
    Error,
}

impl Tone {
    /// Its colour, and the icon a notice shows.
    pub fn look(self) -> (Color, &'static str) {
        use super::Health;
        match self {
            Tone::Plain | Tone::Info => (Color::Selection, "info"),
            Tone::Question => (Color::Selection, "circle-help"),
            Tone::Success => (Health::Up.color(), "circle-check"),
            Tone::Warning => (Health::Busy.color(), "triangle-alert"),
            Tone::Error => (Health::Down.color(), "circle-x"),
        }
    }
}

/// The room left around a dialog inside the app's window, and its title's height.
const MARGIN: i32 = 24;
const HEAD: i32 = 44;

thread_local! {
    /// The app's window: dialogs open inside it.
    static HOST: std::cell::RefCell<Option<DoubleWindow>> = const { std::cell::RefCell::new(None) };
}

/// Dialogs open inside `win` (the app's window) from now on.
pub fn set_host(win: &DoubleWindow) {
    HOST.with_borrow_mut(|h| *h = Some(win.clone()));
}

/// The app's window, when it's on screen: a dialog opens inside it.
fn host() -> Option<DoubleWindow> {
    HOST.with_borrow(|h| h.clone())
        .filter(|w| !w.was_deleted() && w.shown() && w.visible())
}

/// A dialog: a panel in the middle of the app's window, over the window blurred and dimmed (clicks and
/// keys don't reach it while the dialog is open). Not a window of its own: those can't be placed on
/// Wayland, tiling desktops tile them, and another could be opened behind it. Use it as the window it
/// holds; dropping it takes it away. With the app's window hidden (in the tray) it is a window of its own.
pub struct Dialog {
    win: DoubleWindow,
    /// The blurred window under the panel, with the panel's title and close button.
    scrim: Option<Window>,
    /// What had the keyboard before it opened: it gets it back.
    focus: Option<fltk::widget::Widget>,
    /// The room above its contents: its title row, or (with no title) the panel's top edge.
    head: i32,
    /// What the scrim shows behind the panel, and the window as it was, to fade from and to.
    fade: Rc<RefCell<Fade>>,
}

/// The window behind a dialog: blurred once it's open, and as it was (to fade from as it opens and to
/// as it closes, kept only while animations are on).
#[derive(Default)]
struct Fade {
    /// The window blurred and dimmed.
    under: Option<RgbImage>,
    /// The window as it was (RGB, `size` pixels).
    sharp: Option<Vec<u8>>,
    size: (i32, i32),
    /// What shows while it fades (None: `under`).
    now: Option<RgbImage>,
}

impl Fade {
    /// Shows the window `blur` of the way (0 to 1) from as it was to blurred; `frost` is `under`'s
    /// pixels, `buf` room for the mix.
    fn at(&mut self, blur: f32, frost: &[u8], buf: &mut Vec<u8>, shown: (i32, i32)) {
        let Some(sharp) = self.sharp.as_ref().filter(|s| s.len() == frost.len()) else {
            return;
        };
        super::anim::blend(sharp, frost, blur, buf);
        self.now = RgbImage::new(buf, self.size.0, self.size.1, fltk::enums::ColorDepth::Rgb8)
            .ok()
            .map(|mut i| {
                i.scale(shown.0, shown.1, false, true);
                i
            });
    }
}

impl std::ops::Deref for Dialog {
    type Target = DoubleWindow;
    fn deref(&self) -> &DoubleWindow {
        &self.win
    }
}

impl std::ops::DerefMut for Dialog {
    fn deref_mut(&mut self) -> &mut DoubleWindow {
        &mut self.win
    }
}

#[cfg(windows)]
fn fix_child_window_stacking(win: &DoubleWindow, scrim: &Window) {
    if win.was_deleted() || scrim.was_deleted() || !win.shown() || !scrim.shown() {
        return;
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, WS_CLIPSIBLINGS,
    };
    let h_win = win.raw_handle();
    let h_scrim = scrim.raw_handle();
    if !h_win.is_null() && !h_scrim.is_null() {
        unsafe {
            let style_scrim = GetWindowLongPtrW(h_scrim as _, GWL_STYLE) as u32;
            SetWindowLongPtrW(h_scrim as _, GWL_STYLE, (style_scrim | WS_CLIPSIBLINGS) as _);
            let style_win = GetWindowLongPtrW(h_win as _, GWL_STYLE) as u32;
            SetWindowLongPtrW(h_win as _, GWL_STYLE, (style_win | WS_CLIPSIBLINGS) as _);
            SetWindowPos(
                h_win as _,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

impl Dialog {
    /// Shows it and gives the keyboard to its first control.
    pub fn open(&mut self) {
        let Some(mut s) = self.scrim.clone() else {
            self.win.show();
            #[cfg(windows)]
            super::set_window_titlebar_theme(&self.win, super::system_is_dark());
            return;
        };
        if let Some(mut host) = host() {
            if self.win.parent().is_none() {
                // Over the blurred window: added after it, so drawn (and stacked) above it.
                host.add(&self.win);
                let head = self.head;
                place(&host, &mut self.win, head);
                place_close(&mut s, &self.win, head);
            }
        }
        s.show();
        self.win.show();
        #[cfg(windows)]
        fix_child_window_stacking(&self.win, &s);
        self.animate(true);
        #[cfg(windows)]
        fix_child_window_stacking(&self.win, &s);
        self.win.redraw();
        // Closing fades back to the window as it is then, not as it was.
        if !super::anim::on() {
            self.fade.borrow_mut().sharp = None;
        }
        let _ = first_focus(&self.win.as_group().expect("a window is a group"));
    }

    /// Fades the window behind in (blurred) as the panel rises into place, or out as it sinks away.
    fn animate(&mut self, opening: bool) {
        let Some(scrim) = self.scrim.clone() else { return };
        if !super::anim::on() || self.win.parent().is_none() {
            return;
        }
        let Some(frost) = self.fade.borrow().under.as_ref().map(RgbImage::to_rgb_data) else {
            return;
        };
        let (fade, head, mut win, mut scrim) = (self.fade.clone(), self.head, self.win.clone(), scrim);
        let (x, y) = (win.x(), win.y());
        let mut buf = Vec::new();
        super::anim::play(if opening { super::anim::IN } else { super::anim::OUT }, |p| {
            let blur = if opening { p } else { 1.0 - p };
            fade.borrow_mut().at(blur, &frost, &mut buf, (scrim.w(), scrim.h()));
            let drop = if opening {
                super::anim::mix(16, 0, p)
            } else {
                super::anim::mix(0, 10, p)
            };
            win.set_pos(x, y + drop);
            place_close(&mut scrim, &win, head);
            scrim.redraw();
            win.redraw();
        });
        fade.borrow_mut().now = None;
        if !opening {
            win.set_pos(x, y);
            place_close(&mut scrim, &win, head);
        }
        win.redraw();
    }

    /// What makes it `h` high later (its contents grew or shrank), kept in the middle of the window.
    pub fn resizer(&self) -> impl Fn(i32) + Clone + 'static {
        let (win, scrim, head) = (self.win.clone(), self.scrim.clone(), self.head);
        move |h| {
            let mut win = win.clone();
            win.resize(win.x(), win.y(), win.w(), h);
            if let (Some(host), true) = (host(), win.parent().is_some()) {
                place(&host, &mut win, head);
            }
            if let Some(mut s) = scrim.clone() {
                place_close(&mut s, &win, head);
                #[cfg(windows)]
                fix_child_window_stacking(&win, &s);
                s.redraw();
            }
            win.redraw();
        }
    }

    /// Hides it (and what it covers comes back).
    pub fn close(&mut self) {
        if self.win.shown() {
            self.animate(false);
        }
        self.win.hide();
        if let Some(s) = &mut self.scrim {
            s.hide();
        }
        if let Some(h) = host() {
            h.clone().redraw();
        }
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        if !self.win.was_deleted() {
            self.win.hide();
            app::delete_widget(self.win.clone());
        }
        if let Some(s) = self.scrim.take().filter(|s| !s.was_deleted()) {
            app::delete_widget(s);
        }
        if let Some(mut f) = self.focus.take().filter(|f| !f.was_deleted()) {
            let _ = f.take_focus();
        }
        if let Some(h) = host() {
            h.clone().redraw();
        }
    }
}

/// The close button (the scrim's first child) at the panel's top right corner.
fn place_close(scrim: &mut Window, win: &DoubleWindow, head: i32) {
    let (px, py, pw, _) = panel(win, head);
    if let Some(mut b) = scrim.child(0) {
        b.resize(px + pw - 38, py + 8, 28, 28);
    }
}

/// Gives the keyboard to the first control in `g` that takes it (as a new window does). True when one did.
fn first_focus(g: &fltk::group::Group) -> bool {
    focusable(g, false)
}

/// The first (or last) control in `g` that takes the keyboard gets it.
fn focusable(g: &fltk::group::Group, last: bool) -> bool {
    let n = g.children();
    let order: Vec<i32> = if last { (0..n).rev().collect() } else { (0..n).collect() };
    for i in order {
        let Some(mut c) = g.child(i) else { continue };
        if !c.visible() || !c.active() {
            continue;
        }
        match c.as_group() {
            Some(sub) if c.as_window().is_none() => {
                if focusable(&sub, last) {
                    return true;
                }
            }
            Some(_) => {}
            None => {
                if c.take_focus().is_ok() && c.has_focus() {
                    return true;
                }
            }
        }
    }
    false
}

/// The app's window as drawn now, blurred and dimmed (what a dialog sits on), and as it is (RGB, to fade
/// from; only while animations are on).
fn frosted(host: &DoubleWindow) -> Fade {
    let Some(shot) = super::popover::backdrop(host, 0, 0, host.w(), host.h()) else {
        return Fade::default();
    };
    let (w, h) = (shot.data_w() as usize, shot.data_h() as usize);
    let depth = shot.depth() as usize;
    let data = shot.to_rgb_data();
    let (r, g, b) = Color::Background.to_rgb();
    let dark = u32::from(r) + u32::from(g) + u32::from(b) < 384;
    let out = frost(&data, w, h, depth, if dark { 0.34 } else { 0.22 });
    let under = RgbImage::new(&out, w as i32, h as i32, fltk::enums::ColorDepth::Rgb8)
        .ok()
        .map(|mut img| {
            img.scale(host.w(), host.h(), false, true);
            img
        });
    let sharp = super::anim::on().then(|| match depth {
        3 => data,
        _ => data
            .chunks(depth.max(1))
            .flat_map(|px| {
                [
                    px[0],
                    px.get(1).copied().unwrap_or(px[0]),
                    px.get(2).copied().unwrap_or(px[0]),
                ]
            })
            .collect(),
    });
    Fade {
        under,
        sharp,
        size: (w as i32, h as i32),
        now: None,
    }
}

/// `data` (`w` x `h`, `depth` bytes a pixel) blurred and darkened by `dim` (0 to 1): made small, blurred
/// there and made big again, so it costs little however big the window is. RGB out.
pub(super) fn frost(data: &[u8], w: usize, h: usize, depth: usize, dim: f32) -> Vec<u8> {
    const F: usize = 8;
    let (sw, sh) = ((w / F).max(1), (h / F).max(1));
    // Small: each pixel the average of the block it stands for.
    let mut small = vec![0f32; sw * sh * 3];
    for sy in 0..sh {
        for sx in 0..sw {
            let mut sum = [0f32; 3];
            let mut n = 0f32;
            for y in sy * F..((sy + 1) * F).min(h) {
                for x in sx * F..((sx + 1) * F).min(w) {
                    let at = (y * w + x) * depth;
                    if at + 2 < data.len() {
                        for (k, s) in sum.iter_mut().enumerate() {
                            *s += f32::from(data[at + k]);
                        }
                        n += 1.0;
                    }
                }
            }
            for k in 0..3 {
                small[(sy * sw + sx) * 3 + k] = sum[k] / n.max(1.0);
            }
        }
    }
    // Blurred: a box across and down, four times (close to a Gaussian, with no edges left).
    let mut tmp = small.clone();
    for _ in 0..4 {
        box_pass(&small, &mut tmp, sw, sh, true);
        box_pass(&tmp, &mut small, sw, sh, false);
    }
    // Big again, smoothly, and darker.
    let keep = 1.0 - dim.clamp(0.0, 1.0);
    let mut out = vec![0u8; w * h * 3];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) / F as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) / F as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            for k in 0..3 {
                let p = |xx: usize, yy: usize| small[(yy * sw + xx) * 3 + k];
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                out[(y * w + x) * 3 + k] = ((top * (1.0 - ty) + bottom * ty) * keep).round() as u8;
            }
        }
    }
    out
}

/// One box blur (radius 3) along rows (`across`) or columns.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, across: bool) {
    const R: isize = 3;
    let (len, lines) = if across { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| if across { line * w + i } else { i * w + line };
    for line in 0..lines {
        for i in 0..len {
            let mut sum = [0f32; 3];
            for d in -R..=R {
                let j = (i as isize + d).clamp(0, len as isize - 1) as usize;
                for (k, s) in sum.iter_mut().enumerate() {
                    *s += src[at(line, j) * 3 + k];
                }
            }
            for k in 0..3 {
                dst[at(line, i) * 3 + k] = sum[k] / (2 * R + 1) as f32;
            }
        }
    }
}

/// A soft shadow for a panel `w` x `h` (rounded by `r`), `spread` px around it: an RGBA picture as big
/// as the panel and its shadow.
fn shadow(w: i32, h: i32, r: i32, spread: i32) -> Option<RgbImage> {
    let (sw, sh) = (w + spread * 2, h + spread * 2);
    let mut px = vec![0u8; (sw * sh * 4) as usize];
    for y in 0..sh {
        for x in 0..sw {
            // How far outside the rounded panel (shifted down a little, as light comes from above).
            let (cx, cy) = (x - spread, y - spread - 4);
            let qx = (cx.max(r).min(w - r)) as f32;
            let qy = (cy.max(r).min(h - r)) as f32;
            let d = ((cx as f32 - qx).hypot(cy as f32 - qy) - r as f32).max(0.0);
            let a = (1.0 - d / spread as f32).clamp(0.0, 1.0);
            px[((y * sw + x) * 4 + 3) as usize] = (a * a * 110.0) as u8;
        }
    }
    RgbImage::new(&px, sw, sh, fltk::enums::ColorDepth::Rgba8).ok()
}

/// A dialog `w` x `h` (its contents), titled `title` (no title row when empty). Build its widgets next
/// (they go in it), then `run` it. Closing it (its X, Escape) cancels.
pub fn window(w: i32, h: i32, title: &str) -> (Dialog, Answer) {
    window_tone(w, h, title, Tone::Plain)
}

/// `window`, for something that is news, a question, a success, a warning or an error.
pub fn window_tone(w: i32, h: i32, title: &str, tone: Tone) -> (Dialog, Answer) {
    crate::crash::step(&format!("dialog {title}"));
    let answer: Answer = Rc::new(Cell::new(None));
    let focus = app::focus().map(|f| f.as_base_widget());
    // Made on its own (so what is built next goes in it, and nothing after its `end` does), and put in
    // the app's window as it opens.
    let mut win = DoubleWindow::default().with_size(w, h).with_label(title);
    win.set_xclass("streamerz");
    win.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    let Some(host) = host() else {
        // No window to open in (it's in the tray): a window of its own, in the middle of the screen.
        win.make_modal(true);
        let (sx, sy, sw, sh) = app::screen_xywh(0);
        win.set_pos(sx + (sw - w) / 2, sy + (sh - h) / 2);
        return (
            Dialog {
                win,
                scrim: None,
                focus,
                head: 0,
                fade: Rc::default(),
            },
            answer,
        );
    };
    win.set_frame(FrameType::FlatBox);
    win.set_color(Color::Background);
    // Tab goes round its controls, never out to the window behind.
    win.handle(|w, ev| match ev {
        Event::KeyDown if app::event_key() == Key::Tab => {
            let back = app::event_state().contains(fltk::enums::EventState::Shift);
            w.parent().is_some() && focusable(&w.as_group().expect("a window is a group"), back)
        }
        _ => false,
    });
    let head = head_for(title);
    let fade = Rc::new(RefCell::new(frosted(&host)));
    let shade: Rc<std::cell::RefCell<Option<Shade>>> = Rc::default();
    let (tint, _) = tone.look();
    let title = title.to_string();
    let was = fltk::group::Group::try_current();
    host.begin();
    let mut scrim = Window::new(0, 0, host.w(), host.h(), None);
    scrim.set_frame(FrameType::NoBox);
    scrim.set_color(Color::Background);
    // The blur and the panel first, then the close button on them.
    scrim.super_draw_first(false);
    let mut x = fltk::button::Button::new(0, 0, 28, 28, None);
    x.set_frame(FrameType::NoBox);
    x.set_down_frame(FrameType::NoBox);
    x.clear_visible_focus();
    x.set_tooltip(tr("dialogs.close"));
    if title.is_empty() {
        x.hide();
    }
    x.draw(|b| {
        let hover = app::belowmouse::<fltk::widget::Widget>().is_some_and(|w| w.as_widget_ptr() == b.as_widget_ptr());
        // It repaints alone on a hover: it clears what it drew before.
        draw::set_draw_color(if hover {
            super::rows::selected(Color::Background)
        } else {
            Color::Background
        });
        draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), 6);
        let ink = if hover { Color::Foreground } else { super::muted() };
        if let Some(mut i) = super::icons::icon("x", ink, 16) {
            i.draw(b.x() + 6, b.y() + 6, 16, 16);
        }
    });
    x.handle(|b, ev| match ev {
        Event::Enter | Event::Leave => {
            b.redraw();
            true
        }
        _ => false,
    });
    x.set_callback({
        let mut win = win.clone();
        move |_| win.do_callback()
    });
    scrim.end();
    host.end();
    fltk::group::Group::set_current(was.as_ref());
    scrim.draw({
        let (title, win, fade) = (title.clone(), win.clone(), fade.clone());
        move |s| {
            if s.damage_type() == Damage::Child {
                return;
            }
            let (sw, sh) = (s.w(), s.h());
            let f = fade.borrow();
            match f.now.as_ref().or(f.under.as_ref()) {
                Some(img) if img.w() == sw && img.h() == sh => img.clone().draw(0, 0, sw, sh),
                Some(img) => {
                    let mut i = img.clone();
                    i.scale(sw, sh, false, true);
                    i.draw(0, 0, sw, sh);
                }
                None => draw::draw_rect_fill(0, 0, sw, sh, Color::Background.darker()),
            }
            let (px, py, pw, ph) = panel(&win, head);
            {
                let mut cache = shade.borrow_mut();
                if cache.as_ref().is_none_or(|(size, _)| *size != (pw, ph)) {
                    *cache = shadow(pw, ph, RADIUS + 2, SHADOW).map(|i| ((pw, ph), i));
                }
                if let Some((_, img)) = cache.as_ref() {
                    img.clone()
                        .draw(px - SHADOW, py - SHADOW, pw + SHADOW * 2, ph + SHADOW * 2);
                }
            }
            let back = Color::Background;
            draw::set_draw_color(back);
            draw::draw_rounded_rectf(px, py, pw, ph, RADIUS + 2);
            let border = match tone {
                Tone::Plain => Color::color_average(Color::Foreground, back, 0.2),
                _ => Color::color_average(tint, back, 0.55),
            };
            draw::set_draw_color(border);
            draw::draw_rounded_rect(px, py, pw, ph, RADIUS + 2);
            if !title.is_empty() {
                draw::set_font(Font::HelveticaBold, 15);
                draw::set_draw_color(Color::Foreground);
                draw::draw_text2(
                    &title.replace('@', "@@"),
                    px + 16,
                    py + 4,
                    pw - 64,
                    HEAD - 4,
                    Align::Left | Align::Inside | Align::Clip,
                );
            }
        }
    });
    // Nothing behind it is reached while it's open: clicks, the wheel, keys and shortcuts stop here.
    // Escape cancels, as closing a window did.
    scrim.handle({
        let mut win = win.clone();
        move |_, ev| match ev {
            Event::KeyDown | Event::Shortcut if app::event_key() == Key::Escape => {
                win.do_callback();
                true
            }
            Event::Push
            | Event::Released
            | Event::Drag
            | Event::Move
            | Event::Enter
            | Event::MouseWheel
            | Event::KeyDown
            | Event::KeyUp
            | Event::Shortcut
            | Event::DndEnter
            | Event::DndDrag
            | Event::DndRelease
            | Event::DndLeave
            | Event::Paste => true,
            _ => false,
        }
    });
    scrim.resize_callback({
        let (host, mut win) = (host.clone(), win.clone());
        move |s, x, y, w, h| {
            // Over the whole window, whatever FLTK made of it when the window was resized, and the
            // panel in its middle.
            if (x, y, w, h) != (0, 0, host.w(), host.h()) {
                s.resize(0, 0, host.w(), host.h());
                return;
            }
            place(&host, &mut win, head);
            place_close(s, &win, head);
            #[cfg(windows)]
            fix_child_window_stacking(&win, s);
            s.redraw();
            win.redraw();
        }
    });
    (
        Dialog {
            win,
            scrim: Some(scrim),
            focus,
            head,
            fade,
        },
        answer,
    )
}

/// A shadow drawn for a panel of this size.
type Shade = ((i32, i32), RgbImage);

/// How far a dialog's shadow spreads.
const SHADOW: i32 = 22;

/// The room above a dialog's contents: its title row, or with no title as much as below them, so the
/// panel's top edge and round corners show (the contents are square and would cover them).
fn head_for(title: &str) -> i32 {
    if title.is_empty() {
        RADIUS
    } else {
        HEAD
    }
}

/// The panel a dialog's contents sit in (x, y, w, h): its title row above them, and room below them
/// for its round corners.
fn panel(win: &DoubleWindow, head: i32) -> (i32, i32, i32, i32) {
    panel_around((win.x(), win.y(), win.w(), win.h()), head)
}

/// `panel` for contents at `(x, y, w, h)`. A pixel wider on each side than them: they never cover its
/// border.
fn panel_around((x, y, w, h): (i32, i32, i32, i32), head: i32) -> (i32, i32, i32, i32) {
    (x - 1, y - head, w + 2, h + head + RADIUS)
}

/// Puts `win` in the middle of `host` (below its title row), made smaller to fit when it doesn't.
fn place(host: &DoubleWindow, win: &mut DoubleWindow, head: i32) {
    let (room_w, room_h) = (host.w() - MARGIN * 2, host.h() - MARGIN * 2 - head - RADIUS);
    let (mut w, mut h) = (win.w(), win.h());
    if w > room_w || h > room_h {
        // Its contents follow (a window's children are moved and sized with it).
        win.make_resizable(true);
        w = w.min(room_w.max(200));
        h = h.min(room_h.max(120));
    }
    let x = ((host.w() - w) / 2).max(0);
    let y = ((host.h() - (h + head + RADIUS)) / 2).max(0) + head;
    win.resize(x, y, w, h);
}

/// OK and Cancel at the right of a row; Enter is OK when `enter`.
pub fn ok_cancel(bar: &mut Flex, answer: &Answer, enter: bool) {
    Frame::default();
    let mut ok = button(bar, tr("dialogs.ok"), 90);
    let mut cancel = button(bar, tr("dialogs.cancel"), 90);
    if enter {
        ok.set_shortcut(Shortcut::from_key(Key::Enter));
    }
    cancel.set_shortcut(Shortcut::from_key(Key::Escape));
    ok.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(true))
    });
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
}

/// Show and wait for an answer; `check` may refuse an OK (it says why itself). Hides the dialog.
pub fn run(win: &mut Dialog, answer: &Answer, mut check: impl FnMut() -> bool) -> bool {
    run_layout(win, answer, &mut check, |_| {})
}

/// Reflow responsive contents on existing UI events, without an idle timer.
fn run_layout(
    win: &mut Dialog,
    answer: &Answer,
    mut check: impl FnMut() -> bool,
    mut layout: impl FnMut(&mut Dialog),
) -> bool {
    layout(win);
    win.open();
    loop {
        while win.shown() && answer.get().is_none() {
            if !super::nested_wait(None) {
                break;
            }
            layout(win);
        }
        match answer.get() {
            Some(true) if !check() => answer.set(None),
            a => {
                win.close();
                return a.unwrap_or(false);
            }
        }
    }
}

/// `text` broken into lines no wider than `room` by `width` (words stay whole; a word wider than the room
/// is cut). Newlines in `text` are kept.
pub(super) fn wrap(text: &str, room: i32, width: impl Fn(&str) -> i32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ').filter(|w| !w.is_empty()) {
            let joined = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if width(&joined) <= room {
                line = joined;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            // A word too wide for a line alone: cut where it stops fitting.
            let mut rest = word;
            while width(rest) > room && rest.chars().count() > 1 {
                let mut cut = rest
                    .char_indices()
                    .map(|(i, _)| i)
                    .skip(1)
                    .take_while(|&i| width(&rest[..i]) <= room)
                    .last();
                cut = cut.or_else(|| rest.char_indices().nth(1).map(|(i, _)| i));
                let (head, tail) = rest.split_at(cut.unwrap_or(rest.len()));
                out.push(head.to_string());
                rest = tail;
            }
            line = rest.to_string();
        }
        out.push(line);
    }
    out
}

/// A small message: an icon in its tone's colour, the text wrapped to fit, and its buttons (`no`
/// is None for a plain notice). True when `yes` was pressed.
fn notice(tone: Tone, text: &str, no: Option<&str>, yes: &str) -> bool {
    let buttons: Vec<&str> = no.into_iter().chain([yes]).collect();
    notice_of(tone, text, &buttons, buttons.len() - 1, no.is_some()) == Some(buttons.len() - 1)
}

/// `notice` with any number of buttons: the one picked, or None when it is closed (or Escape, when
/// `escapable`). Enter picks `default`.
fn notice_of(tone: Tone, text: &str, buttons: &[&str], default: usize, escapable: bool) -> Option<usize> {
    const PAD: i32 = 22;
    const PIC: i32 = 40;
    const GAP: i32 = 16;
    const LINE: i32 = 19;
    let (tint, icon) = tone.look();
    draw::set_font(Font::Helvetica, 14);
    let button_w = |b: &str| (draw::width(b) as i32 + 56).max(90);
    let bar_w: i32 = buttons.iter().map(|b| button_w(b) + 8).sum();
    // As wide as its longest line wants (within reason), and wide enough for its buttons.
    let one_line = text.split('\n').map(|l| draw::width(l) as i32).max().unwrap_or(0);
    let room = one_line.clamp(220, 420).max(bar_w - PIC - GAP);
    let lines = wrap(text, room, |t| draw::width(t) as i32);
    let w = PAD * 2 + PIC + GAP + room;
    let body = (lines.len() as i32 * LINE).max(PIC);
    // The panel has room above and below the contents already (for its corners): PAD in all.
    let (top, bottom) = (PAD - head_for(""), PAD - RADIUS);
    let h = top + body + 18 + ROW + bottom;
    let (mut win, answer) = window_tone(w, h, "", tone);
    let mut col = Flex::new(PAD, top, w - PAD * 2, h - top - bottom, None).column();
    col.set_pad(18);
    let mut top = Flex::default().row();
    top.set_pad(GAP);
    let mut pic = Frame::default();
    pic.draw(move |f| {
        draw::set_draw_color(Color::color_average(tint, Color::Background, 0.18));
        draw::draw_pie(f.x(), f.y(), PIC, PIC, 0.0, 360.0);
        if let Some(mut i) = super::icons::icon(icon, tint, 22) {
            i.draw(f.x() + 9, f.y() + 9, 22, 22);
        }
    });
    top.fixed(&pic, PIC);
    // Wrapped here, not by FLTK, so the dialog is sized for exactly these lines.
    let mut words = Frame::default()
        .with_label(&lines.join("\n").replace('@', "@@"))
        .with_align(Align::Left | Align::Inside | Align::Top);
    top.end();
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    Frame::default();
    let picked: Rc<Cell<usize>> = Rc::default();
    for (i, label) in buttons.iter().enumerate() {
        let mut b = button(&mut bar, label, button_w(label));
        if i == default {
            b.set_shortcut(Shortcut::from_key(Key::Enter));
        } else if escapable && i == 0 {
            b.set_shortcut(Shortcut::from_key(Key::Escape));
        }
        b.set_callback({
            let (a, picked) = (answer.clone(), picked.clone());
            move |_| {
                picked.set(i);
                a.set(Some(true));
            }
        });
    }
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    let mut size = None;
    run_layout(
        &mut win,
        &answer,
        || true,
        |win| {
            let available = host().map(|host| (host.w(), host.h()));
            if size == Some(available) {
                return;
            }
            size = Some(available);
            let width = available.map_or(w, |(width, _)| w.min((width - MARGIN * 2).max(200)));
            draw::set_font(Font::Helvetica, 14);
            let lines = wrap(text, (width - PAD * 2 - PIC - GAP).max(1), |s| draw::width(s) as i32);
            let body = (lines.len() as i32 * LINE).max(PIC);
            let inset = PAD - head_for("");
            let height = inset + body + 18 + ROW + bottom;
            words.set_label(&lines.join("\n").replace('@', "@@"));
            let (x, y) = (win.x(), win.y());
            win.resize(x, y, width, height);
            col.resize(PAD, inset, width - PAD * 2, body + 18 + ROW);
            col.layout();
            top.layout();
            win.resizer()(height);
        },
    )
    .then(|| picked.get())
}

/// Asks what to do among `choices` (each a button): the index picked, or None when it is closed. The
/// first is the one Escape picks.
pub fn pick_one(text: &str, choices: &[&str]) -> Option<usize> {
    notice_of(Tone::Question, text, choices, 0, true)
}

/// Something to watch out for (it didn't stop anything), with an OK.
pub fn alert(text: &str) {
    notice(Tone::Warning, text, None, tr("dialogs.ok"));
}

/// Something that failed, with an OK.
pub fn error(text: &str) {
    notice(Tone::Error, text, None, tr("dialogs.ok"));
}

/// Something that worked, with an OK.
pub fn success(text: &str) {
    notice(Tone::Success, text, None, tr("dialogs.ok"));
}

/// News, with an OK.
pub fn message(text: &str) {
    notice(Tone::Info, text, None, tr("dialogs.ok"));
}

/// The built-in reference, inside the same shared dialog used by the editors.
pub fn reference(kind: &str, code: u32) {
    let Some(text) = crate::docs::text(kind, code) else {
        return;
    };
    let heading = text
        .lines()
        .next()
        .map(|l| l.trim_start_matches("## ").trim())
        .filter(|l| !l.is_empty());
    let title = match heading {
        Some(h) => format!("{}: {h}", tr("btn.help")),
        None => tr("btn.help").to_string(),
    };
    let (mut win, answer) = window(680, 520, &title);
    let mut col = Flex::new(10, 10, 660, 500, None).column();
    col.set_pad(8);
    let mut view = fltk::text::TextDisplay::default();
    super::scroll::view(&mut view);
    let mut buffer = TextBuffer::default();
    buffer.set_text(&crate::docs::readable(text));
    view.set_buffer(buffer);
    view.set_text_font(Font::Helvetica);
    view.wrap_mode(fltk::text::WrapMode::AtBounds, 0);
    let mut bar = Flex::default().row();
    Frame::default();
    let mut close = button(&mut bar, tr("dialogs.ok"), 90);
    close.set_shortcut(Shortcut::from_key(Key::Escape));
    let done = answer.clone();
    close.set_callback(move |_| done.set(Some(true)));
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);
    run(&mut win, &answer, || true);
}

/// A question: true for `yes`, false for `no` (or closing it).
pub fn choice(text: &str, no: &str, yes: &str) -> bool {
    // No second button asked for: a notice, closed by its one button.
    if yes.is_empty() {
        notice(Tone::Info, text, None, no);
        return false;
    }
    notice(Tone::Question, text, Some(no), yes)
}

/// A question about something that can't be undone (deleting, ending): its border and icon are red.
pub fn danger(text: &str, no: &str, yes: &str) -> bool {
    notice(Tone::Error, text, Some(no), yes)
}

pub fn row(label: &str) -> Flex {
    let mut r = Flex::default().row();
    r.set_pad(8);
    let l = Frame::default()
        .with_label(label)
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    r.fixed(&l, 150);
    r
}

/// Edit a JSON value as text; the new value, if OK'd and valid (`valid` may refuse it with a reason).
pub fn json_editor(title: &str, hint: &str, v: &Value, valid: impl Fn(&Value) -> Result<(), String>) -> Option<Value> {
    json_editor_reference(title, hint, v, None, valid)
}

pub fn json_editor_reference(
    title: &str,
    hint: &str,
    v: &Value,
    help_ref: Option<(&'static str, u32)>,
    valid: impl Fn(&Value) -> Result<(), String>,
) -> Option<Value> {
    let (mut win, answer) = window(680, 520, title);
    let mut col = Flex::new(10, 10, 660, 500, None).column();
    col.set_pad(6);
    if !hint.is_empty() {
        let mut h = Frame::default()
            .with_label(&hint.replace('@', "@@"))
            .with_align(Align::Left | Align::Inside | Align::Wrap);
        h.set_label_color(super::muted());
        col.fixed(&h, 40);
    }
    let mut ed = TextEditor::default();
    super::scroll::view(&mut ed);
    let buf = TextBuffer::default();
    ed.set_buffer(buf.clone());
    ed.set_text_font(Font::Courier);
    let mut buf2 = buf.clone();
    buf2.set_text(&serde_json::to_string_pretty(v).unwrap_or_default());
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    if let Some((kind, code)) = help_ref {
        if crate::docs::text(kind, code).is_some() {
            let mut help = button(&mut bar, tr("btn.help"), 90);
            help.set_callback(move |_| reference(kind, code));
        }
    }
    ok_cancel(&mut bar, &answer, false);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);
    let mut out = None;
    let ok = run(&mut win, &answer, || match serde_json::from_str::<Value>(&buf.text()) {
        Ok(v) => match valid(&v) {
            Ok(()) => {
                out = Some(v);
                true
            }
            Err(e) => {
                error(&e);
                false
            }
        },
        Err(e) => {
            error(&trf("dialogs.not_valid_json", &[("error", &e.to_string())]));
            false
        }
    });
    if ok {
        out
    } else {
        None
    }
}

/// Pick one of `items` using the tree picker dialog.
pub fn pick(title: &str, items: &[String]) -> Option<usize> {
    let pick_items: Vec<super::picker::PickItem> = items
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let (cat, item_title) = if let Some((c, n)) = name.split_once(": ") {
                (c.to_string(), n.to_string())
            } else {
                (tr("dialogs.action.general").to_string(), name.clone())
            };
            super::picker::PickItem {
                reference: None,
                id: i,
                title: item_title,
                category: cat,
                description: String::new(),
            }
        })
        .collect();
    super::picker::pick_tree(title, &pick_items)
}

/// An action's settings: name, group, queue and flags. `queues`: (id, name), the default first.
pub fn action(a: &Action, groups: &[String], queues: &[(String, String)]) -> Option<Action> {
    let (mut win, answer) = window(460, 330, tr("dialogs.action.title"));
    let mut col = Flex::new(10, 10, 440, 310, None).column();
    col.set_pad(4);
    let r = row(tr("dialogs.name"));
    let mut name = Input::default();
    name.set_value(&a.name);
    r.end();
    col.fixed(&r, ROW);
    let r = row(tr("dialogs.action.group"));
    let mut group = InputChoice::default();
    super::menu::input_choice(&mut group);
    for g in groups {
        group.add(&g.replace('/', "\\/").replace('&', "&&"));
    }
    group.set_value(&a.group);
    r.end();
    col.fixed(&r, ROW);
    let r = row(tr("dialogs.action.queue"));
    let mut queue = super::fit::choice();
    for (_, n) in queues {
        queue.add_choice(&n.replace('/', "\\/").replace('&', "&&").replace('|', "\\|"));
    }
    let cur = a.queue.clone().unwrap_or_default();
    queue.set_value(queues.iter().position(|(id, _)| *id == cur).unwrap_or(0) as i32);
    r.end();
    col.fixed(&r, ROW);
    let check = |col: &mut Flex, label: &str, on: bool| {
        let c = CheckButton::default().with_label(label);
        c.set_checked(on);
        col.fixed(&c, ROW);
        c
    };
    let enabled = check(&mut col, tr("dialogs.action.enabled"), a.enabled);
    let random = check(&mut col, tr("dialogs.action.random"), a.random_action);
    let concurrent = check(&mut col, tr("dialogs.action.concurrent"), a.concurrent);
    let always = check(&mut col, tr("dialogs.action.always_run"), a.always_run);
    let history = check(
        &mut col,
        tr("dialogs.action.exclude_from_history"),
        a.exclude_from_history,
    );
    let pending = check(
        &mut col,
        tr("dialogs.action.exclude_from_pending"),
        a.exclude_from_pending,
    );
    Frame::default();
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    let ok = run(&mut win, &answer, || {
        let empty = name.value().trim().is_empty();
        if empty {
            alert(tr("dialogs.action.needs_name"));
        }
        !empty
    });
    if !ok {
        return None;
    }
    let mut out = a.clone();
    out.name = name.value().trim().into();
    out.group = group.value().unwrap_or_default().trim().into();
    out.queue = queues.get(queue.value().max(0) as usize).map(|(id, _)| id.clone());
    out.enabled = enabled.is_checked();
    out.random_action = random.is_checked();
    out.concurrent = concurrent.is_checked();
    out.always_run = always.is_checked();
    out.exclude_from_history = history.is_checked();
    out.exclude_from_pending = pending.is_checked();
    Some(out)
}

/// Ask for an import string (pasted, or from a file); its contents.
pub fn import() -> Option<Obj> {
    let (mut win, answer) = window(640, 360, tr("dialogs.import.title"));
    let mut col = Flex::new(10, 10, 620, 340, None).column();
    col.set_pad(6);
    let l = Frame::default()
        .with_label(tr("dialogs.import.hint"))
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    col.fixed(&l, ROW);
    let mut text = MultilineInput::default();
    text.set_wrap(true);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut load = button(&mut bar, &format!("{}...", tr("btn.load_file")), 110);
    ok_cancel(&mut bar, &answer, false);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);
    load.set_callback({
        let mut text = text.clone();
        move |_| {
            let mut fc = dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseFile);
            fc.show();
            let f = fc.filename();
            if f.as_os_str().is_empty() {
                return;
            }
            match std::fs::read_to_string(&f) {
                Ok(s) => text.set_value(s.trim()),
                Err(e) => error(&trf(
                    "dialogs.import.cant_read",
                    &[("file", &f.display().to_string()), ("error", &e.to_string())],
                )),
            }
        }
    });
    let mut out = None;
    let ok = run(&mut win, &answer, || match crate::model::read_export(&text.value()) {
        Ok(x) => {
            out = Some(x);
            true
        }
        Err(e) => {
            error(&trf("dialogs.import.cant_import", &[("error", &e.to_string())]));
            false
        }
    });
    if ok {
        out
    } else {
        None
    }
}

/// Choose actions and the export's details, then show the export string (copy or save it).
/// Pick what to export: whole groups (with their settings and files, such as overlays, even when
/// they hold no action) and single actions. `build` gets the action ids and the group names.
pub fn export(
    actions: &[(String, String, String)],
    groups: &[String],
    selected: Option<&str>,
    build: impl Fn(&[String], &[String], [&str; 4]) -> String + 'static,
) {
    let (mut win, answer) = window(640, 580, tr("dialogs.export.title"));
    let mut col = Flex::new(10, 10, 620, 560, None).column();
    col.set_pad(4);
    let l = Frame::default()
        .with_label(tr("dialogs.export.hint"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&l, 2 * ROW);
    let mut sel_bar = Flex::default().row();
    sel_bar.set_pad(4);
    let mut btn_all = button(&mut sel_bar, tr("dialogs.export.select_all"), 90);
    let mut btn_none = button(&mut sel_bar, tr("dialogs.export.select_none"), 90);
    Frame::default();
    sel_bar.end();
    col.fixed(&sel_bar, ROW);
    let mut list = CheckBrowser::default();
    super::scroll::view(&mut list);
    for g in groups {
        list.add(&trf("dialogs.export.group", &[("name", g)]), false);
    }
    for (id, name, group) in actions {
        let label = if group.is_empty() {
            name.clone()
        } else {
            format!("[{group}] {name}")
        };
        list.add(&label, Some(id.as_str()) == selected);
    }
    btn_all.set_callback({
        let mut list = list.clone();
        move |_| list.check_all()
    });
    btn_none.set_callback({
        let mut list = list.clone();
        move |_| list.check_none()
    });
    let field = |col: &mut Flex, label: &str| {
        let r = row(label);
        let i = Input::default();
        r.end();
        col.fixed(&r, ROW);
        i
    };
    let mut name = field(&mut col, tr("dialogs.name"));
    if let Some((_, n, _)) = actions.iter().find(|(id, _, _)| Some(id.as_str()) == selected) {
        name.set_value(n);
    }
    let author = field(&mut col, tr("dialogs.export.author"));
    let mut version = field(&mut col, tr("dialogs.export.version"));
    version.set_value("1.0.0");
    let description = field(&mut col, tr("dialogs.export.description"));
    let mut out = MultilineInput::default();
    out.set_wrap(true);
    col.fixed(&out, 90);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut make = button(&mut bar, tr("btn.export"), 90);
    let mut copy = button(&mut bar, tr("btn.copy"), 90);
    let mut save = button(&mut bar, &format!("{}...", tr("btn.save_to_file")), 120);
    Frame::default();
    let mut close = button(&mut bar, tr("dialogs.close"), 90);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);
    let ids = actions.iter().map(|(id, _, _)| id.clone()).collect::<Vec<_>>();
    let groups = groups.to_vec();
    make.set_callback({
        let (list, name, mut out) = (list.clone(), name.clone(), out.clone());
        move |_| {
            let ticked: Vec<usize> = (1..=list.nitems())
                .filter(|&i| list.checked(i as i32))
                .map(|i| i - 1)
                .collect();
            let chosen_groups: Vec<String> = ticked.iter().filter_map(|&i| groups.get(i).cloned()).collect();
            let chosen: Vec<String> = ticked
                .iter()
                .filter_map(|&i| ids.get(i.checked_sub(groups.len())?).cloned())
                .collect();
            if chosen.is_empty() && chosen_groups.is_empty() {
                return alert(tr("dialogs.export.tick_at_least_one"));
            }
            let s = build(
                &chosen,
                &chosen_groups,
                [&name.value(), &author.value(), &version.value(), &description.value()],
            );
            out.set_value(&s);
        }
    });
    copy.set_callback({
        let out = out.clone();
        move |_| app::copy(&out.value())
    });
    save.set_callback({
        let (out, name) = (out.clone(), name.clone());
        move |_| {
            if out.value().is_empty() {
                return alert(tr("dialogs.export.export_first"));
            }
            let mut fc = dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseSaveFile);
            fc.set_preset_file(&format!("{}.sb", name.value().trim()));
            fc.show();
            let f = fc.filename();
            if !f.as_os_str().is_empty() {
                if let Err(e) = std::fs::write(&f, out.value()) {
                    error(&trf(
                        "dialogs.export.cant_save",
                        &[("file", &f.display().to_string()), ("error", &e.to_string())],
                    ));
                }
            }
        }
    });
    close.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    run(&mut win, &answer, || true);
}

/// Wait for a key combination to be pressed, as games set a key: SharpHook's key code and modifier
/// mask (see `hotkeys`), or None when cancelled.
pub fn capture_keys() -> Option<(i64, i64)> {
    use crate::hotkeys::{self, ALT, CTRL, META, SHIFT};
    let (mut win, answer) = window(560, 170, tr("dialogs.keys.title"));
    let mut col = Flex::new(10, 10, 540, 150, None).column();
    col.set_pad(6);
    let mut shown = Frame::default().with_label(tr("dialogs.keys.prompt"));
    shown.set_label_size(18);
    shown.set_label_font(Font::HelveticaBold);
    col.fixed(&shown, 50);
    let mut note = Frame::default()
        .with_label(hotkeys::how_keys())
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    note.set_label_size(12);
    col.fixed(&note, 34);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    Frame::default();
    let mut ok = button(&mut bar, tr("dialogs.ok"), 90);
    let mut cancel = button(&mut bar, tr("dialogs.cancel"), 90);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    // Buttons don't take the keys: every key goes to the window.
    for b in [&mut ok, &mut cancel] {
        b.visible_focus(false);
    }
    ok.deactivate();
    let got: Rc<Cell<Option<(i64, i64)>>> = Rc::default();
    ok.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(true))
    });
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    win.handle({
        let got = got.clone();
        move |_, ev| {
            if !matches!(ev, fltk::enums::Event::KeyDown | fltk::enums::Event::Shortcut) {
                return false;
            }
            let st = app::event_state();
            let mut mask = 0;
            for (on, bit) in [
                (Shortcut::Ctrl, CTRL),
                (Shortcut::Alt, ALT),
                (Shortcut::Shift, SHIFT),
                (Shortcut::Meta, META),
            ] {
                if st.contains(on) {
                    mask |= bit;
                }
            }
            let k = app::event_key().bits();
            // A modifier alone: wait for the key.
            if (0xffe1..=0xffee).contains(&k) {
                return true;
            }
            match hotkeys::by_fltk(k) {
                Some(key) => {
                    got.set(Some((key.code, mask)));
                    shown.set_label(&hotkeys::label(key.code, mask).replace('&', "&&").replace('@', "@@"));
                    ok.activate();
                }
                None => shown.set_label(tr("dialogs.keys.unusable")),
            }
            true
        }
    });
    run(&mut win, &answer, || true).then(|| got.get()).flatten()
}

/// Wait for a click with the mouse button wanted (SharpHook's number, 0: any button), as `capture_keys`
/// waits for keys; None when cancelled.
pub fn capture_button(now: i64) -> Option<i64> {
    use crate::hotkeys::{button_name, from_fltk_button};
    let (mut win, answer) = window(420, 170, tr("dialogs.mouse.title"));
    let mut col = Flex::new(10, 10, 400, 150, None).column();
    col.set_pad(6);
    // The whole box takes the click, whichever button it is (the left one too).
    let mut target = Frame::default().with_label(tr("dialogs.mouse.prompt"));
    target.set_frame(fltk::enums::FrameType::BorderBox);
    target.set_label_size(16);
    target.set_label_font(Font::HelveticaBold);
    let mut shown = Frame::default().with_label(button_name(now));
    col.fixed(&shown, ROW);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut any = button(&mut bar, tr("input.any_button"), 110);
    Frame::default();
    let ok = button(&mut bar, tr("dialogs.ok"), 90);
    let mut cancel = button(&mut bar, tr("dialogs.cancel"), 90);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    let got = Rc::new(Cell::new(now));
    target.handle({
        let (got, mut shown) = (got.clone(), shown.clone());
        move |_, ev| {
            if ev != fltk::enums::Event::Push {
                return false;
            }
            if let Some(n) = from_fltk_button(app::event_button()) {
                got.set(n);
                shown.set_label(button_name(n));
            }
            true
        }
    });
    any.set_callback({
        let got = got.clone();
        move |_| {
            got.set(0);
            shown.set_label(button_name(0));
        }
    });
    let mut ok = ok;
    ok.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(true))
    });
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    run(&mut win, &answer, || true).then(|| got.get())
}

/// A value as one line of text: strings as they are, the rest as JSON.
fn shown(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

/// Text typed as a value: a number, true/false, null, a list or an object when it reads as one,
/// else text.
fn typed(t: &str) -> Value {
    match serde_json::from_str::<Value>(t.trim()) {
        Ok(v) if !v.is_string() => v,
        _ => Value::String(t.to_string()),
    }
}

/// Asks for a name and a value; both, if OK'd with a name.
fn name_value(title: &str, name: &str, value: &str) -> Option<(String, String)> {
    let (mut win, answer) = window(460, 110, title);
    let mut col = Flex::new(10, 10, 440, 90, None).column();
    col.set_pad(6);
    let r = row(tr("dialogs.name"));
    let mut n = Input::default();
    n.set_value(name);
    r.end();
    col.fixed(&r, ROW);
    let r = row(tr("dialogs.value"));
    let mut v = Input::default();
    v.set_value(value);
    r.end();
    col.fixed(&r, ROW);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    let _ = if name.is_empty() {
        n.take_focus()
    } else {
        v.take_focus()
    };
    let ok = run(&mut win, &answer, || {
        if n.value().trim().is_empty() {
            alert(tr("dialogs.name_required"));
            false
        } else {
            true
        }
    });
    ok.then(|| (n.value().trim().to_string(), v.value()))
}

/// Variables as a table of names and values. `edit`: they can be added, changed and removed, and
/// the result comes back if OK'd; else it only shows them. JSON is a button away, for power users.
pub fn values(title: &str, hint: &str, o: &Obj, edit: bool) -> Option<Obj> {
    let (mut win, answer) = window(620, 460, title);
    let mut col = Flex::new(10, 10, 600, 440, None).column();
    col.set_pad(6);
    if !hint.is_empty() {
        let mut h = Frame::default()
            .with_label(&hint.replace('@', "@@"))
            .with_align(Align::Left | Align::Inside | Align::Wrap);
        h.set_label_color(super::muted());
        col.fixed(&h, 36);
    }
    super::header(&mut col, &[200], &[tr("dialogs.name"), tr("dialogs.value")]);
    let mut list = super::rows::Rows::new(if edit {
        tr("dialogs.values.empty_edit")
    } else {
        tr("dialogs.values.empty")
    });
    list.set_column_widths(&[200]);
    let vars = Rc::new(std::cell::RefCell::new(o.clone()));
    let fill = {
        let (vars, list) = (vars.clone(), list.clone());
        move || {
            let mut list = list.clone();
            let keep = list.value();
            list.clear();
            for (k, v) in vars.borrow().iter() {
                list.add(&format!(
                    "{}\t{}",
                    k.replace('\t', " "),
                    shown(v).replace(['\t', '\n'], " ")
                ));
            }
            list.select(keep);
        }
    };
    fill();
    let labels: &[&str] = if edit {
        &[tr("btn.add"), tr("btn.edit"), tr("btn.delete")]
    } else {
        &[]
    };
    let b = if edit {
        super::grid(&mut col, labels, 1)
    } else {
        Vec::new()
    };
    list.set_menu(&b);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut json = button(
        &mut bar,
        if edit {
            tr("btn.edit_json")
        } else {
            tr("dialogs.values.view_json")
        },
        120,
    );
    if edit {
        ok_cancel(&mut bar, &answer, false);
    } else {
        Frame::default();
        let mut close = button(&mut bar, tr("dialogs.close"), 90);
        let a = answer.clone();
        close.set_callback(move |_| a.set(Some(false)));
    }
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);

    // The selected variable's name.
    let picked = {
        let (vars, list) = (vars.clone(), list.clone());
        move || {
            let i = usize::try_from(list.value() - 1).ok()?;
            vars.borrow().keys().nth(i).cloned()
        }
    };
    let change = {
        let (vars, fill) = (vars.clone(), fill.clone());
        move |old: Option<String>| {
            let v = old
                .as_ref()
                .and_then(|k| vars.borrow().get(k).map(shown))
                .unwrap_or_default();
            let title = if old.is_some() {
                tr("dialogs.change")
            } else {
                tr("btn.add")
            };
            let Some((k, v)) = name_value(title, old.as_deref().unwrap_or(""), &v) else {
                return;
            };
            let mut vars = vars.borrow_mut();
            if let Some(old) = old.filter(|o| *o != k) {
                vars.remove(&old);
            }
            vars.insert(k, typed(&v));
            drop(vars);
            fill();
        }
    };
    if edit {
        let mut b = b.into_iter();
        let (mut add, mut ed, mut del) = (b.next().unwrap(), b.next().unwrap(), b.next().unwrap());
        add.set_callback({
            let change = change.clone();
            move |_| change(None)
        });
        ed.set_callback({
            let (change, picked) = (change.clone(), picked.clone());
            move |_| {
                if let Some(k) = picked() {
                    change(Some(k));
                }
            }
        });
        del.set_callback({
            let (vars, fill, picked) = (vars.clone(), fill.clone(), picked.clone());
            move |_| {
                if let Some(k) = picked() {
                    vars.borrow_mut().remove(&k);
                    fill();
                }
            }
        });
        list.set_callback({
            let (change, picked) = (change.clone(), picked.clone());
            move |_| {
                if app::event_clicks() {
                    if let Some(k) = picked() {
                        change(Some(k));
                    }
                }
            }
        });
    }
    json.set_callback({
        let (vars, fill, title) = (vars.clone(), fill.clone(), title.to_string());
        move |_| {
            let now = Value::Object(vars.borrow().clone());
            let got = json_editor(&title, "", &now, |v| {
                if v.is_object() {
                    Ok(())
                } else {
                    Err(tr("dialogs.values.need_object").into())
                }
            });
            if let (true, Some(Value::Object(o))) = (edit, got) {
                *vars.borrow_mut() = o;
                fill();
            }
        }
    });
    let ok = run(&mut win, &answer, || true);
    let out = vars.borrow().clone();
    (ok && edit).then_some(out)
}

#[cfg(test)]
mod wrap_tests {
    use super::wrap;

    /// Seven pixels a character, like a font.
    fn w(t: &str) -> i32 {
        t.chars().count() as i32 * 7
    }

    #[test]
    fn lines_fit_and_keep_every_word() {
        let text = "Streamerz v1.2.3 is out (this is v1.2.2). Update now, or later if you are streaming?";
        for room in [70, 140, 210, 420] {
            let lines = wrap(text, room, w);
            assert!(lines.iter().all(|l| w(l) <= room), "{room}: {lines:?}");
            assert_eq!(lines.join(" "), text);
        }
    }

    #[test]
    fn newlines_stay_and_blank_lines_hold_their_place() {
        assert_eq!(wrap("one\n\ntwo", 700, w), ["one", "", "two"]);
        assert_eq!(wrap("", 700, w), [""]);
    }

    #[test]
    fn a_word_too_wide_is_cut_never_overflowing() {
        let lines = wrap("see /home/user/.config/streamerz/streamerz.json now", 105, w);
        assert!(lines.iter().all(|l| w(l) <= 105), "{lines:?}");
        assert_eq!(
            lines.join("").replace(' ', ""),
            "see/home/user/.config/streamerz/streamerz.jsonnow"
        );
    }

    #[test]
    fn a_room_narrower_than_a_letter_still_ends() {
        assert_eq!(wrap("abc", 1, w).concat(), "abc");
    }
}

#[cfg(test)]
mod panel_tests {
    use super::{head_for, panel_around, RADIUS};

    /// Whether the point `(x, y)` is at least `gap` px inside the rounded rect `(px, py, pw, ph)`.
    fn inside((x, y): (i32, i32), (px, py, pw, ph): (i32, i32, i32, i32), r: i32, gap: f32) -> bool {
        let cx = x.clamp(px + r, px + pw - r);
        let cy = y.clamp(py + r, py + ph - r);
        let from_centre = ((x - cx) as f32).hypot((y - cy) as f32);
        x >= px && y >= py && x <= px + pw && y <= py + ph && from_centre <= r as f32 - gap
    }

    #[test]
    fn a_dialogs_border_shows_all_round_with_or_without_a_title() {
        // The contents are square and painted over the panel: none of their corners may reach its
        // border (a notice, with no title, covered its top edge and its round corners).
        for title in ["", "OBS connection"] {
            let (x, y, w, h) = (300, 200, 400, 120);
            let p = panel_around((x, y, w, h), head_for(title));
            assert!(p.1 < y, "{title:?}: the panel's top edge is above the contents");
            for corner in [(x, y), (x + w - 1, y), (x, y + h - 1), (x + w - 1, y + h - 1)] {
                assert!(inside(corner, p, RADIUS + 2, 0.5), "{title:?}: {corner:?} covers {p:?}");
            }
        }
    }
}

#[cfg(test)]
mod look {
    /// By eye, on a display (`DISPLAY=:99 cargo test look -- --ignored`): each dialog waits for Enter.
    #[test]
    #[ignore]
    fn dialogs() {
        let _app = crate::ui::app();
        crate::ui::theme(true, None);
        let only = std::env::var("DLG").unwrap_or_default();
        if only == "1" {
            super::message("Short one.");
        } else if only == "2" {
            super::alert("Can't save /home/someone/.config/streamerz/a-very-long-folder-name-that-has-no-spaces/streamerz.json: permission denied (os error 13), so nothing was written. Try another folder.");
        } else {
            let long = "Streamerz v1.2.3 is out (this is v1.2.2).\n\nIt was downloaded and goes in place the next time Streamerz starts. Update now, or later if you are streaming?";
            super::choice(long, "Later", "Update and restart now");
        }
    }
}
