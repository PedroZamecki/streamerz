//! Native window: actions, settings and the log, in tabs. Lives hidden in the tray; while hidden its native
//! window is destroyed and nothing redraws.

mod accounts;
mod actions;
mod chat;
mod dialogs;
mod forms;
mod lists;
pub mod picker;

use crate::engine::{Engine, UiCmd};
use crate::{status, Config};
use fltk::button::{Button, CheckButton};
use fltk::enums::{Align, ColorDepth, Event, Font, Shortcut};
use fltk::frame::Frame;
use fltk::group::{Flex, Tabs};
use fltk::image::RgbImage;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay};
use fltk::window::Window;
use fltk::{app, dialog};
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

/// The engine changed the config: redraw what shows it, if the window is up.
pub fn changed() {
    if SHOWN.load(Ordering::Relaxed) {
        if let Some(ui) = UI.get() {
            ui.send(Msg::Changed);
        }
    }
}

/// New chat: refresh the chat tab, if the window is up.
pub fn chat() {
    if SHOWN.load(Ordering::Relaxed) {
        if let Some(ui) = UI.get() {
            ui.send(Msg::Chat);
        }
    }
}

/// A line was logged: refresh the log, if the window is up.
pub fn logged() {
    if SHOWN.load(Ordering::Relaxed) {
        if let Some(ui) = UI.get() {
            ui.send(Msg::Log);
        }
    }
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

fn theme(dark: bool) {
    if dark {
        app::background(45, 45, 48);
        app::background2(30, 30, 32);
        app::foreground(225, 225, 225);
        app::set_selection_color(70, 110, 170);
    } else {
        app::background(192, 192, 192);
        app::background2(255, 255, 255);
        app::foreground(0, 0, 0);
        app::set_selection_color(0, 0, 128);
    }
}

/// A line of text; `bold` for section titles.
fn text(col: &mut Flex, label: &str, bold: bool) -> Frame {
    let mut f = Frame::default()
        .with_label(&label.replace('@', "@@"))
        .with_align(Align::Left | Align::Inside);
    if bold {
        f.set_label_font(Font::HelveticaBold);
    }
    col.fixed(&f, ROW - 4);
    f
}

fn check(col: &mut Flex, label: &str, on: bool) -> CheckButton {
    let c = CheckButton::default().with_label(label);
    c.set_checked(on);
    col.fixed(&c, ROW);
    c
}

fn button(row: &mut Flex, label: &str, w: i32) -> Button {
    let b = Button::default().with_label(label);
    row.fixed(&b, w);
    b
}

fn tab(tabs: &Tabs, label: &str) -> Flex {
    let mut col = Flex::new(tabs.x(), tabs.y() + 25, tabs.w(), tabs.h() - 25, None)
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
    let (c_dark, c_hidden, c_autostart, c_instance) = {
        let c = cfg.borrow();
        (c.dark, c.start_hidden, c.autostart, c.instance_name.clone())
    };
    let _ = UI.set(tx);
    theme(c_dark);
    let mut win = Window::default().with_size(W, H).with_label(&crate::version());
    win.set_xclass("streamerz");
    use crate::tray::{ICON, SIZE};
    if let Ok(icon) = RgbImage::new(ICON, SIZE, SIZE, ColorDepth::Rgba8) {
        win.set_icon(Some(icon));
    }
    let tabs = Tabs::new(5, 5, W - 10, H - 50, None);

    let acts = actions::tab(&tabs, cfg.clone(), cmds.clone());
    let views = lists::tabs(&tabs, &cfg, &cmds);

    // Settings
    let mut col = tab(&tabs, "Settings");
    text(&mut col, "Window", true);
    let dark = check(&mut col, "Dark theme", c_dark);
    let hidden = check(&mut col, "Start hidden in the tray", c_hidden);
    let autostart = check(
        &mut col,
        if cfg!(windows) {
            "Start with Windows"
        } else {
            "Start when I log in"
        },
        c_autostart,
    );
    text(&mut col, "This instance", true);
    let r = dialogs::row("Name (GetInfo)");
    let mut instance = fltk::input::Input::default();
    instance.set_value(&c_instance);
    r.end();
    col.fixed(&r, ROW);
    Frame::default();
    text(
        &mut col,
        &format!("Settings file: {}", crate::config_path().display()),
        false,
    );
    col.end();

    // Log
    let col = tab(&tabs, "Log");
    let mut view = TextDisplay::default();
    let buf = TextBuffer::default();
    view.set_buffer(buf.clone());
    col.end();
    tabs.end();
    let mut log = Log {
        view,
        buf,
        seen: u64::MAX,
        shown: 0,
    };

    let mut bar = Flex::new(10, H - 40, W - 20, 32, None).row();
    let mut save = button(&mut bar, "Save", 90);
    save.set_shortcut(Shortcut::Ctrl | 's');
    save.set_tooltip("Save and apply (Ctrl+S)");
    let mut saved = Frame::default().with_align(Align::Left | Align::Inside);
    let mut hide = button(&mut bar, "Hide", 90);
    let mut quit = button(&mut bar, "Quit", 90);
    bar.end();
    win.end();
    win.resizable(&tabs);
    let views = Rc::new(views);
    tabs.clone().set_callback({
        let views = views.clone();
        move |t| views.refresh(t)
    });

    dark.clone().set_callback({
        let win = win.clone();
        move |b| {
            theme(b.is_checked());
            win.clone().redraw();
        }
    });
    let show = !c_hidden;
    save.set_callback(move |_| {
        cfg.send_modify(|c| {
            c.dark = dark.is_checked();
            c.start_hidden = hidden.is_checked();
            c.autostart = autostart.is_checked();
            c.instance_name = instance.value().trim().to_string();
        });
        let c = cfg.borrow();
        let done = crate::save(&c).and_then(|()| crate::autostart(c.autostart));
        saved.set_label(&match done {
            Ok(()) => "Saved, applied.".to_string(),
            Err(e) => format!("Can't save: {e}"),
        });
    });
    quit.set_callback(|_| std::process::exit(0));
    let close = |w: &mut Window| {
        SHOWN.store(false, Ordering::Relaxed);
        w.hide();
    };
    hide.set_callback({
        let mut win = win.clone();
        move |_| close(&mut win)
    });
    let tray_ok = Rc::new(Cell::new(true));
    win.set_callback({
        let tray_ok = tray_ok.clone();
        move |w| {
            if app::event() == Event::Close {
                if tray_ok.get() {
                    close(w);
                } else {
                    std::process::exit(0);
                }
            }
        }
    });

    // The tabs are filled only when shown: filling a list measures its text, which opens the display
    // and loads the fonts (~14 MB) even with the window hidden.
    let open = |win: &mut Window, log: &mut Log| {
        if !win.visible() {
            SHOWN.store(true, Ordering::Relaxed);
            acts.borrow_mut().refresh();
            views.refresh(&tabs);
            win.show();
            log.refresh();
        }
    };
    if show {
        open(&mut win, &mut log);
    }
    // Not app.run() or app.wait(): with no window shown they return at once (busy loop / exit).
    // wait_for blocks until a tray message or UI event arrives.
    loop {
        let _ = app::wait_for(1e9);
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
                Msg::Changed => {
                    acts.borrow_mut().refresh();
                    views.refresh(&tabs);
                }
                Msg::Input => {
                    let asked = QUESTIONS
                        .lock()
                        .map(|mut q| std::mem::take(&mut *q))
                        .unwrap_or_default();
                    for (title, prompt, text, answer) in asked {
                        let _ = answer.send(ask(&title, &prompt, &text));
                    }
                }
            }
        }
    }
}
