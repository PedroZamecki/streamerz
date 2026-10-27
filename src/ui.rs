//! Native window: actions, settings and the log, in tabs. Lives hidden in the tray; while hidden its native
//! window is destroyed and nothing redraws.

mod accounts;
mod actions;
mod chat;
mod conns;
mod dialogs;
pub(crate) mod emotes;
mod forms;
mod grid;
pub(crate) mod icons;
pub(crate) mod img;
mod lists;
mod nav;
pub mod picker;
mod rows;
mod settings;

use crate::engine::{Engine, UiCmd};
use crate::{status, Config};
use fltk::button::Button;
use fltk::enums::{Align, Color, ColorDepth, Event, Font, FrameType, Shortcut};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::image::RgbImage;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay};
use fltk::window::Window;
use fltk::{app, dialog, draw};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::{mpsc::UnboundedSender, oneshot, watch};

#[derive(Clone, Copy)]
pub enum Msg {
    /// Tray click: show or hide the window.
    Toggle,
    /// No tray icon (e.g. no StatusNotifier host on Linux): keep the window up, closing it quits.
    NoTray,
    /// New log lines (sent only while the window is up).
    Log,
    /// A Get Input sub-action wants an answer (see `input`).
    Input,
    /// The engine changed the config (an action's state, a global...).
    Changed,
    /// New chat lines (sent only while the window is up).
    Chat,
    /// A picture from the web is on disk now (see `img`).
    Image,
    /// An update is in place: restart into it.
    Restart,
}

const ROW: i32 = 26;
const W: i32 = 1000;
const H: i32 = 700;

/// Where `logged` wakes the window, and whether it's up (no wakeups while hidden).
static UI: OnceLock<app::Sender<Msg>> = OnceLock::new();
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Get Input's questions: title, prompt, starting text, and where the answer goes.
type Question = (String, String, String, oneshot::Sender<Option<String>>);
static QUESTIONS: Mutex<Vec<Question>> = Mutex::new(Vec::new());

/// Ask in a dialog (Get Input). The answer, or None when cancelled (or there's no window).
pub fn input(title: String, prompt: String, text: String) -> oneshot::Receiver<Option<String>> {
    let (tx, rx) = oneshot::channel();
    if let (Some(ui), Ok(mut q)) = (UI.get(), QUESTIONS.lock()) {
        q.push((title, prompt, text, tx));
        ui.send(Msg::Input);
    }
    rx
}

/// Yes/no questions from the engine: the text, the two buttons (no, yes), and where the answer goes.
type Confirm = (String, &'static str, &'static str, oneshot::Sender<bool>);
static CONFIRMS: Mutex<Vec<Confirm>> = Mutex::new(Vec::new());

/// Asks `text` with buttons `no` and `yes` (shown even while the window hides); true for `yes`.
pub fn confirm(text: String, no: &'static str, yes: &'static str) -> oneshot::Receiver<bool> {
    let (tx, rx) = oneshot::channel();
    if let Ok(mut q) = CONFIRMS.lock() {
        q.push((text, no, yes, tx));
    }
    // Before the window is made, it asks when it is.
    if let Some(ui) = UI.get() {
        ui.send(Msg::Input);
    }
    rx
}

/// Saves what the window keeps, starts the new build and quits (after an update).
pub fn restart() {
    if let Some(ui) = UI.get() {
        ui.send(Msg::Restart);
    }
}

/// A modal text question. Not dialog::input: on Wayland it focuses its input before the window
/// exists and crashes FLTK.
fn ask(title: &str, prompt: &str, text: &str) -> Option<String> {
    let mut win = Window::default().with_size(420, 130).with_label(title);
    win.set_xclass("streamerz");
    let mut col = Flex::new(10, 10, 400, 110, None).column();
    col.set_pad(6);
    let p = Frame::default()
        .with_label(&prompt.replace('@', "@@"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    let mut input = fltk::input::Input::default();
    input.set_value(text);
    col.fixed(&input, ROW);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    Frame::default();
    let mut ok = button(&mut bar, "OK", 90);
    let mut cancel = button(&mut bar, "Cancel", 90);
    bar.end();
    col.fixed(&bar, ROW + 4);
    let _ = p;
    col.end();
    win.end();
    win.make_modal(true);
    let answer = Rc::new(Cell::new(None::<bool>));
    ok.set_shortcut(Shortcut::from_key(fltk::enums::Key::Enter));
    ok.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(true))
    });
    cancel.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    win.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    win.show();
    while win.shown() && answer.get().is_none() {
        if !app::wait() {
            break;
        }
    }
    win.hide();
    answer.get().unwrap_or(false).then(|| input.value())
}

/// Run `f` on the engine's thread.
fn tell(cmds: &UnboundedSender<UiCmd>, f: impl FnOnce(&Rc<Engine>) + Send + 'static) {
    let _ = cmds.send(Box::new(f));
}

/// Run `f` on the engine's thread and wait for its answer. The engine never waits on the window,
/// so this can't deadlock.
fn query<T: Send + 'static>(
    cmds: &UnboundedSender<UiCmd>,
    f: impl FnOnce(&Rc<Engine>) -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    tell(cmds, move |e| drop(tx.send(f(e))));
    rx.recv().ok()
}

/// Wake the window with `m`, if it's up (no wakeups while hidden).
fn wake(m: Msg) {
    if SHOWN.load(Ordering::Relaxed) {
        if let Some(ui) = UI.get() {
            ui.send(m);
        }
    }
}

/// The engine changed the config: redraw what shows it, if the window is up.
pub fn changed() {
    wake(Msg::Changed);
}

/// New chat: refresh the chat tab, if the window is up.
pub fn chat() {
    wake(Msg::Chat);
}

/// A line was logged: refresh the log, if the window is up.
pub fn logged() {
    wake(Msg::Log);
}

pub fn app() -> app::App {
    app::App::default().with_scheme(app::Scheme::Gtk)
}

/// Show a startup error; there's no console on Windows.
#[allow(dead_code)] // ponytail: first used by the servers (port taken), phase 4.
pub fn fatal(msg: &str) -> ! {
    eprintln!("{msg}");
    dialog::alert_default(msg);
    std::process::exit(1);
}

/// Flat, rounded and quiet (zinc colours, like shadcn/ui): every button, field, list and tab is drawn
/// by the two box functions below, so the whole window follows.
fn theme(dark: bool) {
    if dark {
        app::background(24, 24, 27);
        app::background2(9, 9, 11);
        app::foreground(244, 244, 245);
        app::set_selection_color(59, 130, 246);
    } else {
        app::background(244, 244, 245);
        app::background2(255, 255, 255);
        app::foreground(9, 9, 11);
        app::set_selection_color(37, 99, 235);
    }
    // Raised: a button, a tab, a menu button. Sunken: a field, a list, a check box.
    // Small boxes (check boxes) get small corners.
    fn r(w: i32, h: i32) -> i32 {
        (w.min(h) / 5).clamp(1, 6)
    }
    fn up(x: i32, y: i32, w: i32, h: i32, c: Color) {
        let fg = Color::Foreground;
        draw::set_draw_color(Color::color_average(c, fg, 0.9));
        draw::draw_rounded_rectf(x, y, w, h, r(w, h));
        draw::set_draw_color(Color::color_average(c, fg, 0.78));
        draw::draw_rounded_rect(x, y, w, h, r(w, h));
    }
    fn down(x: i32, y: i32, w: i32, h: i32, c: Color) {
        draw::set_draw_color(c);
        draw::draw_rounded_rectf(x, y, w, h, r(w, h));
        draw::set_draw_color(Color::color_average(c, Color::Foreground, 0.7));
        draw::draw_rounded_rect(x, y, w, h, r(w, h));
    }
    // No dotted focus boxes: fields show their cursor, lists their selection.
    app::set_visible_focus(false);
    app::set_menu_linespacing(8);
    app::set_scrollbar_size(12);
    for f in [
        FrameType::UpBox,
        FrameType::ThinUpBox,
        FrameType::GtkUpBox,
        FrameType::GtkThinUpBox,
    ] {
        app::set_frame_type_cb(f, up, 2, 2, 4, 4);
    }
    for f in [
        FrameType::DownBox,
        FrameType::ThinDownBox,
        FrameType::GtkDownBox,
        FrameType::GtkThinDownBox,
    ] {
        app::set_frame_type_cb(f, down, 2, 2, 4, 4);
    }
}

/// Quiet text: hints, empty lists and column titles.
fn muted() -> Color {
    Color::color_average(Color::Foreground, Color::Background2, 0.5)
}

/// How a connection is doing, shown the same way everywhere: an icon and a colour.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Health {
    Up,
    /// Waiting for the stream to start.
    Standby,
    Busy,
    Off,
    Down,
    Locked,
}

impl Health {
    pub(crate) fn color(self) -> Color {
        match self {
            Health::Up => Color::from_rgb(0x3c, 0xb3, 0x71),
            Health::Standby => Color::from_rgb(0x64, 0xb5, 0xf6),
            Health::Busy | Health::Locked => Color::from_rgb(0xe0, 0xa8, 0x30),
            Health::Off => Color::from_rgb(0x99, 0x99, 0x99),
            Health::Down => Color::from_rgb(0xe0, 0x60, 0x50),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Health::Up => "circle-check",
            Health::Standby => "moon",
            Health::Busy => "loader-circle",
            Health::Off => "circle-off",
            Health::Down => "circle-alert",
            Health::Locked => "lock",
        }
    }

    /// `f` shows `text` in this state: its icon before it, both in its colour.
    pub(crate) fn show(self, f: &mut impl WidgetExt, text: &str) {
        f.set_label(&text.replace('@', "@@"));
        f.set_label_color(self.color());
        f.set_image(icons::before_label(self.icon(), self.color(), 14));
        f.set_align(Align::Left | Align::Inside | Align::Clip | Align::ImageNextToText);
    }
}

/// `text` in an empty field that isn't being typed in, saying what goes there.
fn placeholder(i: &mut fltk::input::Input, text: &'static str) {
    i.draw(move |i| {
        if i.value().is_empty() && !i.has_focus() {
            draw::set_font(Font::Helvetica, i.text_size());
            draw::set_draw_color(muted());
            draw::draw_text2(text, i.x() + 6, i.y(), i.w() - 12, i.h(), Align::Left | Align::Clip);
        }
    });
}

/// Column titles over a list with these column `widths`.
fn header(col: &mut Flex, widths: &'static [i32], titles: &'static [&'static str]) {
    let mut f = Frame::default();
    f.draw(move |f| {
        draw::set_font(Font::HelveticaBold, 12);
        draw::set_draw_color(muted());
        let mut x = f.x() + 12;
        for (i, t) in titles.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(f.w());
            draw::draw_text2(t, x, f.y(), w, f.h(), Align::Left | Align::Clip);
            x += w;
        }
    });
    col.fixed(&f, 16);
}

fn button(row: &mut Flex, label: &str, w: i32) -> Button {
    let mut b = Button::default().with_label(label);
    let wide = with_icon(&mut b);
    row.fixed(&b, w + if wide { 20 } else { 0 });
    b
}

/// The icon of a button's label (call again after relabelling it) (Add, Edit, Delete...) before it, if it has one; true if so.
pub(crate) fn with_icon(b: &mut Button) -> bool {
    match icon_for(&b.label()) {
        Some(name) => {
            b.set_image(icons::before_label(name, button_ink(), 14));
            b.set_align(Align::ImageNextToText);
            true
        }
        None => {
            b.set_image(None::<fltk::image::SvgImage>);
            false
        }
    }
}

/// A button icon's colour.
fn button_ink() -> Color {
    Color::color_average(Color::Foreground, Color::Background, 0.75)
}

/// The icon for a button labelled `label`, if it has one.
fn icon_for(label: &str) -> Option<&'static str> {
    Some(match label.trim_end_matches(['.', '…']) {
        "Add" => "plus",
        "Edit" | "Edit JSON" => "pencil",
        "Delete" => "trash-2",
        "On/Off" => "power",
        "Import" | "Load file" => "download",
        "Export" | "Save to file" => "upload",
        "Connect" | "Connect All" => "plug",
        "Disconnect" | "Disconnect All" => "unplug",
        "Test" => "play",
        "Templates" => "layout-template",
        "Save" => "save",
        "Retry" | "Refresh" | "Reset" | "Check" => "rotate-cw",
        "Quit" | "Logout" => "log-out",
        "Login" => "log-in",
        "Hide" => "eye-off",
        "Duplicate" | "Copy" | "Copy line" | "Copy Code" | "Copy redirect URL" | "Copy path" => "copy",
        "Clear" => "eraser",
        "Advanced" => "sliders-horizontal",
        "Settings" => "settings",
        "Fill with test names" => "users",
        "Open folder" => "folder-open",
        "Open" | "Open file" | "Open profile" | "Open Browser" => "external-link",
        _ => return None,
    })
}

/// The menu a right click opens: `buttons`' actions, the inactive ones greyed. Runs the one picked.
pub(crate) fn context_menu(buttons: &[Button]) {
    use fltk::menu::{MenuButton, MenuFlag};
    let mut m = MenuButton::new(app::event_x(), app::event_y(), 0, 0, None);
    for b in buttons {
        let l = b.label();
        let l = if l.is_empty() {
            b.tooltip().unwrap_or_default()
        } else {
            l
        };
        let flag = if b.active() {
            MenuFlag::Normal
        } else {
            MenuFlag::Inactive
        };
        let i = m.add(&l.replace('/', "\\/").replace('&', "&&"), Shortcut::None, flag, {
            let mut b = b.clone();
            move |_| b.do_callback()
        });
        if let (Some(mut item), Some(name)) = (m.at(i), icon_for(&l)) {
            item.add_image(icons::before_label(name, button_ink(), 14), true);
        }
    }
    if m.size() > 0 {
        m.popup();
    }
    MenuButton::delete(m);
}

/// Buttons in a grid `rows` high that fills `col`'s width, all the same width; where their labels don't
/// fit, they show only their icons (the label in the tooltip).
fn grid(col: &mut Flex, labels: &[&str], rows: i32) -> Vec<Button> {
    let mut g = fltk::group::Group::default();
    let out: Vec<Button> = labels.iter().map(|l| Button::default().with_label(l)).collect();
    g.end();
    col.fixed(&g, rows * (ROW + 2) + (rows - 1) * 6);
    let labels: Vec<String> = labels.iter().map(|l| (*l).to_string()).collect();
    let place = {
        let out = out.clone();
        move |x: i32, y: i32, w: i32, h: i32| {
            let n = out.len() as i32;
            let cols = (n + rows - 1) / rows;
            let cw = (w - (cols - 1) * 8) / cols.max(1);
            let rh = (h - (rows - 1) * 6) / rows;
            // Measured once the window shows (measuring loads the fonts); a guess before.
            let shown = SHOWN.load(Ordering::Relaxed);
            if shown {
                draw::set_font(Font::Helvetica, app::font_size());
            }
            let fits = labels.iter().all(|l| {
                let text = if shown {
                    draw::width(l) as i32
                } else {
                    l.len() as i32 * 7
                };
                text + 14 + 7 + 20 <= cw
            });
            for (i, (mut b, l)) in out.iter().cloned().zip(&labels).enumerate() {
                let (r, c) = (i as i32 / cols, i as i32 % cols);
                b.resize(x + c * (cw + 8), y + r * (rh + 6), cw, rh);
                match icon_for(l).filter(|_| !fits) {
                    Some(name) => {
                        b.set_label("");
                        b.set_image(icons::icon(name, button_ink(), 16));
                        b.set_align(Align::Center | Align::ImageNextToText);
                        b.set_tooltip(l);
                    }
                    None => {
                        b.set_label(l);
                        with_icon(&mut b);
                    }
                }
            }
        }
    };
    place(g.x(), g.y(), g.w(), g.h());
    g.resize_callback(move |_, x, y, w, h| place(x, y, w, h));
    out
}

/// A page: `label` is its name in the sidebar.
fn tab(tabs: &Wizard, label: &str) -> Flex {
    let mut col = Flex::new(tabs.x(), tabs.y(), tabs.w(), tabs.h(), None)
        .column()
        .with_label(label);
    col.set_margin(10);
    col.set_pad(4);
    col
}

/// The Log tab: appends only the new lines; rebuilds after a gap or when far behind.
struct Log {
    view: TextDisplay,
    buf: TextBuffer,
    seen: u64,
    shown: usize,
}

impl Log {
    fn refresh(&mut self) {
        let (mut full, mut append) = (None, None);
        status(|s| {
            if s.logged == self.seen {
                return;
            }
            let new = s.logged.wrapping_sub(self.seen) as usize;
            if self.seen == u64::MAX || new > s.log.len() || self.shown + new > 2 * crate::LOG_LINES {
                full = Some(s.log.iter().map(String::as_str).collect::<Vec<_>>().join("\n"));
                self.shown = s.log.len();
            } else {
                let skip = s.log.len() - new;
                append = Some(s.log.iter().skip(skip).fold(String::new(), |a, l| a + "\n" + l));
                self.shown += new;
            }
            self.seen = s.logged;
        });
        if let Some(full) = &full {
            self.buf.set_text(full);
        }
        if let Some(append) = &append {
            // The first line has no newline before it.
            self.buf
                .append(if self.buf.length() == 0 { &append[1..] } else { append });
        }
        if full.is_some() || append.is_some() {
            self.view.scroll(self.view.count_lines(0, self.buf.length(), true), 0);
        }
    }
}

pub fn run(
    _app: app::App,
    cfg: Arc<watch::Sender<Config>>,
    cmds: UnboundedSender<UiCmd>,
    tx: app::Sender<Msg>,
    rx: app::Receiver<Msg>,
) {
    let (c_dark, c_hidden) = {
        let c = cfg.borrow();
        (c.dark, c.start_hidden)
    };
    let _ = UI.set(tx);
    if CONFIRMS.lock().is_ok_and(|q| !q.is_empty()) {
        tx.send(Msg::Input);
    }
    theme(c_dark);
    // Pictures drawn bigger than they are come out smooth, not blocky.
    RgbImage::set_scaling_algorithm(fltk::image::RgbScaling::Bilinear);
    fltk::image::Image::set_scaling_algorithm(fltk::image::RgbScaling::Bilinear);
    let mut win = Window::default().with_size(W, H).with_label(&crate::version());
    win.set_xclass("streamerz");
    use crate::tray::{ICON, SIZE};
    if let Ok(icon) = RgbImage::new(ICON, SIZE, SIZE, ColorDepth::Rgba8) {
        win.set_icon(Some(icon));
    }
    // The pages, one at a time, picked in the sidebar on their left.
    let nav_w = nav::width(&cfg);
    let mut tabs = Wizard::new(nav_w, 0, W - nav_w, H - 46, None);
    tabs.set_frame(FrameType::FlatBox);

    let acts = actions::tab(&tabs, cfg.clone(), cmds.clone());
    let views = lists::tabs(&tabs, &cfg, &cmds);

    settings::tab(&tabs, &cfg, &cmds);

    // Log
    let col = tab(&tabs, "Log");
    let mut view = TextDisplay::default();
    let buf = TextBuffer::default();
    view.set_buffer(buf.clone());
    col.end();
    tabs.end();
    let sidebar = nav::sidebar(H - 46, &tabs, &cfg);
    let mut log = Log {
        view,
        buf,
        seen: u64::MAX,
        shown: 0,
    };

    let mut bar = Flex::new(10, H - 40, W - 20, 32, None).row();
    bar.set_pad(8);
    let mut save = button(&mut bar, "Save", 90);
    save.set_shortcut(Shortcut::Ctrl | 's');
    save.set_tooltip("Save and apply (Ctrl+S)");
    let mut conns = Button::default().with_align(Align::Left | Align::Inside);
    conns.set_frame(fltk::enums::FrameType::NoBox);
    bar.fixed(&conns, 240);
    let mut saved = Frame::default().with_align(Align::Left | Align::Inside);
    let mut hide = button(&mut bar, "Hide", 90);
    let mut quit = button(&mut bar, "Quit", 90);
    bar.end();
    win.end();
    win.resizable(&tabs);
    // Back as it was left: size and tab.
    {
        let c = cfg.borrow();
        if c.window.w >= 600 && c.window.h >= 400 {
            win.set_size(c.window.w, c.window.h);
        }
        if let Some(p) = nav::page(&tabs, &c.window.tab) {
            tabs.set_current_widget(&p);
        }
    }
    // Kept as it changes, written with the next save (and when the window hides or streamerz quits).
    let remember = {
        let cfg = cfg.clone();
        let cmds = cmds.clone();
        move |w: &Window| {
            cfg.send_if_modified(|c| {
                (c.window.w, c.window.h) = (w.w(), w.h());
                false
            });
            let _ = crate::save(&cfg.borrow());
            // Waited for: Quit exits right after.
            let _ = query(&cmds, |e| {
                e.save_chat_history();
                e.save_community();
            });
        }
    };
    // The status bar's "3/5 connected"; a click lists each connection.
    let status_bar = {
        let (cfg, cmds) = (cfg.clone(), cmds.clone());
        let conns = conns.clone();
        move || {
            let mut conns = conns.clone();
            let (label, all) = conns::summary(&cfg, &cmds);
            conns.set_tooltip("Each connection, how it's doing, and Retry");
            all.show(&mut conns, &label);
            conns.redraw();
        }
    };
    conns.set_callback({
        let (cfg, cmds, tabs, status_bar) = (cfg.clone(), cmds.clone(), tabs.clone(), status_bar.clone());
        move |_| {
            let tabs = tabs.clone();
            let go = move |label: &str| nav::go(&tabs, label);
            conns::popup(&cfg, &cmds, go, status_bar.clone());
        }
    });
    let views = Rc::new(views);
    tabs.clone().set_callback({
        let views = views.clone();
        let cfg = cfg.clone();
        let mut sidebar = sidebar.clone();
        move |t| {
            sidebar.redraw();
            let tab = t.try_current_widget().map(|w| w.label()).unwrap_or_default();
            cfg.send_if_modified(|c| {
                c.window.tab = tab;
                false
            });
            views.refresh(t)
        }
    });

    let show = !c_hidden;
    save.set_callback({
        let cfg = cfg.clone();
        move |_| {
            saved.set_label(&match crate::save(&cfg.borrow()) {
                Ok(()) => "Saved".to_string(),
                Err(e) => format!("Can't save: {e}"),
            });
        }
    });
    quit.set_callback({
        let (win, remember) = (win.clone(), remember.clone());
        move |_| {
            remember(&win);
            std::process::exit(0)
        }
    });
    let restart = remember.clone();
    let close = move |w: &mut Window| {
        remember(w);
        SHOWN.store(false, Ordering::Relaxed);
        w.hide();
        img::forget();
    };
    hide.set_callback({
        let mut win = win.clone();
        let close = close.clone();
        move |_| close(&mut win)
    });
    let tray_ok = Rc::new(Cell::new(true));
    win.set_callback({
        let tray_ok = tray_ok.clone();
        let close = close.clone();
        move |w| {
            if app::event() == Event::Close {
                close(w);
                if !tray_ok.get() {
                    std::process::exit(0);
                }
            }
        }
    });

    let prompted_unlock = Rc::new(Cell::new(false));
    // The tabs are filled only when shown: filling a list measures its text, which opens the display
    // and loads the fonts (~14 MB) even with the window hidden.
    let open = |win: &mut Window, log: &mut Log| {
        if !win.visible() {
            SHOWN.store(true, Ordering::Relaxed);
            acts.borrow_mut().refresh();
            views.refresh(&tabs);
            status_bar();
            win.show();
            log.refresh();
            if crate::secrets::is_locked() && !prompted_unlock.get() {
                prompted_unlock.set(true);
                let cmds = cmds.clone();
                app::add_timeout3(0.25, move |_| {
                    if crate::secrets::is_locked()
                        && dialog::choice2_default(
                            "streamerz couldn't read your saved logins because the system keyring was locked or still starting up.\n\nUnlock your saved logins and reconnect now?",
                            "Later",
                            "Unlock Now",
                            "",
                        ) == Some(1)
                    {
                        accounts::unlock_logins_interactive(&cmds);
                    }
                });
            }
        }
    };
    if show {
        open(&mut win, &mut log);
    }
    // Not app.run() or app.wait(): with no window shown they return at once (busy loop / exit).
    // wait_for blocks until a tray message or UI event arrives.
    loop {
        let _ = app::wait_for(1e9);
        // Pictures come in bursts (the emoji picker asks for hundreds): redraw for them once.
        let mut pictures = false;
        while let Some(m) = rx.recv() {
            match m {
                Msg::Toggle if win.visible() => close(&mut win),
                Msg::Toggle => open(&mut win, &mut log),
                Msg::NoTray => {
                    tray_ok.set(false);
                    hide.deactivate();
                    open(&mut win, &mut log);
                }
                Msg::Log => log.refresh(),
                Msg::Chat => views.chat_refresh(&tabs),
                Msg::Image => pictures = true,
                // Hidden: nothing to redraw (filling a list loads the fonts); opening refreshes.
                Msg::Changed if win.visible() => {
                    status_bar();
                    acts.borrow_mut().refresh();
                    views.refresh(&tabs);
                }
                Msg::Changed => {}
                Msg::Input => {
                    let asked = QUESTIONS
                        .lock()
                        .map(|mut q| std::mem::take(&mut *q))
                        .unwrap_or_default();
                    for (title, prompt, text, answer) in asked {
                        let _ = answer.send(ask(&title, &prompt, &text));
                    }
                    let confirms = CONFIRMS.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
                    for (text, no, yes, answer) in confirms {
                        let _ = answer.send(dialog::choice2_default(&text.replace('@', "@@"), no, yes, "") == Some(1));
                    }
                }
                Msg::Restart => {
                    restart(&win);
                    crate::update::restart();
                }
            }
        }
        if pictures {
            views.images();
        }
    }
}
