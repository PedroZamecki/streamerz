//! Modal dialogs: JSON editor, type picker, the action form, import and export.

use super::{button, ROW};
use crate::model::{Action, Obj};
use fltk::browser::{CheckBrowser, HoldBrowser};
use fltk::button::CheckButton;
use fltk::enums::{Align, Event, Font, Key, Shortcut};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::{Input, MultilineInput};
use fltk::menu::Choice;
use fltk::misc::InputChoice;
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextEditor};
use fltk::window::Window;
use fltk::{app, dialog};
use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;

/// The dialog's answer: None while open, then OK (true) or cancel (false).
pub type Answer = Rc<Cell<Option<bool>>>;

/// A dialog window: centred, modal, closing it cancels.
pub fn window(w: i32, h: i32, title: &str) -> (Window, Answer) {
    let mut win = Window::default().with_size(w, h).with_label(title);
    win.set_xclass("streamerz");
    win.make_modal(true);
    let answer: Answer = Rc::new(Cell::new(None));
    win.set_callback({
        let a = answer.clone();
        move |_| a.set(Some(false))
    });
    (win, answer)
}

/// OK and Cancel at the right of a row; Enter is OK when `enter`.
pub fn ok_cancel(bar: &mut Flex, answer: &Answer, enter: bool) {
    Frame::default();
    let mut ok = button(bar, "OK", 90);
    let mut cancel = button(bar, "Cancel", 90);
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

/// Show and wait for an answer; `check` may refuse an OK (it says why itself). Hides the window.
pub fn run(win: &mut Window, answer: &Answer, mut check: impl FnMut() -> bool) -> bool {
    win.show();
    loop {
        while win.shown() && answer.get().is_none() {
            if !app::wait() {
                break;
            }
        }
        match answer.get() {
            Some(true) if !check() => answer.set(None),
            a => {
                win.hide();
                return a.unwrap_or(false);
            }
        }
    }
}

pub fn row(label: &str) -> Flex {
    let mut r = Flex::default().row();
    let l = Frame::default()
        .with_label(label)
        .with_align(Align::Left | Align::Inside);
    r.fixed(&l, 150);
    r
}

/// Edit a JSON value as text; the new value, if OK'd and valid (`valid` may refuse it with a reason).
pub fn json_editor(title: &str, hint: &str, v: &Value, valid: impl Fn(&Value) -> Result<(), String>) -> Option<Value> {
    let (mut win, answer) = window(680, 520, title);
    let mut col = Flex::new(10, 10, 660, 500, None).column();
    col.set_pad(6);
    if !hint.is_empty() {
        let h = Frame::default()
            .with_label(&hint.replace('@', "@@"))
            .with_align(Align::Left | Align::Inside | Align::Wrap);
        col.fixed(&h, 40);
    }
    let mut ed = TextEditor::default();
    let buf = TextBuffer::default();
    ed.set_buffer(buf.clone());
    ed.set_text_font(Font::Courier);
    let mut buf2 = buf.clone();
    buf2.set_text(&serde_json::to_string_pretty(v).unwrap_or_default());
    let mut bar = Flex::default().row();
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
                dialog::alert_default(&e);
                false
            }
        },
        Err(e) => {
            dialog::alert_default(&format!("Not valid JSON: {e}"));
            false
        }
    });
    if ok {
        out
    } else {
        None
    }
}

/// Pick one of `items` (typing filters them). Its index.
pub fn pick(title: &str, items: &[String]) -> Option<usize> {
    let (mut win, answer) = window(520, 560, title);
    let mut col = Flex::new(10, 10, 500, 540, None).column();
    col.set_pad(6);
    let mut filter = Input::default();
    filter.set_tooltip("Type to filter");
    col.fixed(&filter, ROW);
    let mut list = HoldBrowser::default();
    let mut bar = Flex::default().row();
    ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);
    let shown: Rc<std::cell::RefCell<Vec<usize>>> = Rc::default();
    let mut fill = {
        let (mut list, shown, items) = (list.clone(), shown.clone(), items.to_vec());
        move |f: &str| {
            let f = f.to_lowercase();
            list.clear();
            let mut s = shown.borrow_mut();
            s.clear();
            for (i, x) in items.iter().enumerate().filter(|(_, x)| x.to_lowercase().contains(&f)) {
                // "@." makes the line plain text (no format codes).
                list.add(&format!("@.{x}"));
                s.push(i);
            }
            if !s.is_empty() {
                list.select(1);
            }
        }
    };
    fill("");
    filter.set_trigger(fltk::enums::CallbackTrigger::Changed);
    filter.set_callback({
        let mut fill = fill.clone();
        move |i| fill(&i.value())
    });
    list.set_callback({
        let a = answer.clone();
        move |_| {
            if app::event() == Event::Released && app::event_clicks() {
                a.set(Some(true));
            }
        }
    });
    let ok = run(&mut win, &answer, || list.value() > 0);
    let i = list.value();
    let s = shown.borrow();
    (ok && i > 0).then(|| s.get(i as usize - 1).copied()).flatten()
}

/// An action's settings: name, group, queue and flags. `queues`: (id, name), the default first.
pub fn action(a: &Action, groups: &[String], queues: &[(String, String)]) -> Option<Action> {
    let (mut win, answer) = window(460, 330, "Action");
    let mut col = Flex::new(10, 10, 440, 310, None).column();
    col.set_pad(4);
    let r = row("Name");
    let mut name = Input::default();
    name.set_value(&a.name);
    r.end();
    col.fixed(&r, ROW);
    let r = row("Group");
    let mut group = InputChoice::default();
    for g in groups {
        group.add(&g.replace('/', "\\/").replace('&', "&&"));
    }
    group.set_value(&a.group);
    r.end();
    col.fixed(&r, ROW);
    let r = row("Queue");
    let mut queue = Choice::default();
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
    let enabled = check(&mut col, "Enabled", a.enabled);
    let random = check(
        &mut col,
        "Random: run one sub-action, picked by weight",
        a.random_action,
    );
    let concurrent = check(&mut col, "Concurrent: run outside the queue", a.concurrent);
    let always = check(&mut col, "Always run", a.always_run);
    let history = check(&mut col, "Exclude from history", a.exclude_from_history);
    let pending = check(&mut col, "Exclude from pending", a.exclude_from_pending);
    Frame::default();
    let mut bar = Flex::default().row();
    ok_cancel(&mut bar, &answer, true);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    let ok = run(&mut win, &answer, || {
        let empty = name.value().trim().is_empty();
        if empty {
            dialog::alert_default("The action needs a name.");
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
    let (mut win, answer) = window(640, 360, "Import");
    let mut col = Flex::new(10, 10, 620, 340, None).column();
    col.set_pad(6);
    let l = Frame::default()
        .with_label("Paste a Streamer.bot import string, or load a file that has one:")
        .with_align(Align::Left | Align::Inside);
    col.fixed(&l, ROW);
    let mut text = MultilineInput::default();
    text.set_wrap(true);
    let mut bar = Flex::default().row();
    let mut load = button(&mut bar, "Load file...", 110);
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
                Err(e) => dialog::alert_default(&format!("Can't read {}: {e}", f.display())),
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
            dialog::alert_default(&format!("Can't import this: {e}"));
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
pub fn export(
    actions: &[(String, String)],
    selected: Option<&str>,
    build: impl Fn(&[String], [&str; 4]) -> String + 'static,
) {
    let (mut win, answer) = window(640, 560, "Export");
    let mut col = Flex::new(10, 10, 620, 540, None).column();
    col.set_pad(4);
    let l = Frame::default()
        .with_label("Actions to export (the commands, timers, queues and WebSocket clients/servers their triggers use go with them):")
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&l, 2 * ROW);
    let mut list = CheckBrowser::default();
    for (id, name) in actions {
        list.add(name, Some(id.as_str()) == selected);
    }
    let field = |col: &mut Flex, label: &str| {
        let r = row(label);
        let i = Input::default();
        r.end();
        col.fixed(&r, ROW);
        i
    };
    let mut name = field(&mut col, "Name");
    if let Some((_, n)) = actions.iter().find(|(id, _)| Some(id.as_str()) == selected) {
        name.set_value(n);
    }
    let author = field(&mut col, "Author");
    let mut version = field(&mut col, "Version");
    version.set_value("1.0.0");
    let description = field(&mut col, "Description");
    let mut out = MultilineInput::default();
    out.set_wrap(true);
    col.fixed(&out, 90);
    let mut bar = Flex::default().row();
    let mut make = button(&mut bar, "Export", 90);
    let mut copy = button(&mut bar, "Copy", 90);
    let mut save = button(&mut bar, "Save to file...", 120);
    Frame::default();
    let mut close = button(&mut bar, "Close", 90);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);
    let ids = actions.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    make.set_callback({
        let (list, name, mut out) = (list.clone(), name.clone(), out.clone());
        move |_| {
            let chosen: Vec<String> = (1..=list.nitems() as i32)
                .filter(|&i| list.checked(i))
                .filter_map(|i| ids.get(i as usize - 1).cloned())
                .collect();
            if chosen.is_empty() {
                return dialog::alert_default("Tick at least one action.");
            }
            let s = build(
                &chosen,
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
                return dialog::alert_default("Export first.");
            }
            let mut fc = dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseSaveFile);
            fc.set_preset_file(&format!("{}.sb", name.value().trim()));
            fc.show();
            let f = fc.filename();
            if !f.as_os_str().is_empty() {
                if let Err(e) = std::fs::write(&f, out.value()) {
                    dialog::alert_default(&format!("Can't save {}: {e}", f.display()));
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
