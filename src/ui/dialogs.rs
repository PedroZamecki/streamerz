//! Modal dialogs: JSON editor, type picker, the action form, import and export.

use super::{button, ROW};
use crate::model::{Action, Obj};
use fltk::browser::CheckBrowser;
use fltk::button::CheckButton;
use fltk::enums::{Align, Font, Key, Shortcut};
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
    r.set_pad(8);
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
    bar.set_pad(8);
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

/// Pick one of `items` using the tree picker dialog.
#[allow(dead_code)]
pub fn pick(title: &str, items: &[String]) -> Option<usize> {
    let pick_items: Vec<super::picker::PickItem> = items
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let (cat, item_title) = if let Some((c, n)) = name.split_once(": ") {
                (c.to_string(), n.to_string())
            } else {
                ("General".to_string(), name.clone())
            };
            super::picker::PickItem {
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
    bar.set_pad(8);
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
    bar.set_pad(8);
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
    actions: &[(String, String, String)],
    selected: Option<&str>,
    build: impl Fn(&[String], [&str; 4]) -> String + 'static,
) {
    let (mut win, answer) = window(640, 580, "Export");
    let mut col = Flex::new(10, 10, 620, 560, None).column();
    col.set_pad(4);
    let l = Frame::default()
        .with_label("Actions to export (the commands, timers, queues and WebSocket clients/servers their triggers use go with them):")
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    col.fixed(&l, 2 * ROW);
    let mut sel_bar = Flex::default().row();
    sel_bar.set_pad(4);
    let mut btn_all = button(&mut sel_bar, "Select All", 90);
    let mut btn_none = button(&mut sel_bar, "Select None", 90);
    Frame::default();
    sel_bar.end();
    col.fixed(&sel_bar, ROW);
    let mut list = CheckBrowser::default();
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
    let mut name = field(&mut col, "Name");
    if let Some((_, n, _)) = actions.iter().find(|(id, _, _)| Some(id.as_str()) == selected) {
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
    bar.set_pad(8);
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
    let ids = actions.iter().map(|(id, _, _)| id.clone()).collect::<Vec<_>>();
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

/// Wait for a key combination to be pressed, as games set a key: SharpHook's key code and modifier
/// mask (see `hotkeys`), or None when cancelled.
pub fn capture_keys() -> Option<(i64, i64)> {
    use crate::hotkeys::{self, ALT, CTRL, META, SHIFT};
    let hypr = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.contains("Hyprland"));
    let (mut win, answer) = window(560, if hypr { 230 } else { 170 }, "Press the keys");
    let mut col = Flex::new(10, 10, 540, if hypr { 210 } else { 150 }, None).column();
    col.set_pad(6);
    let mut shown = Frame::default().with_label("Press the key combination (e.g. Ctrl+Shift+F9)");
    shown.set_label_size(18);
    shown.set_label_font(Font::HelveticaBold);
    col.fixed(&shown, 50);
    let how = if cfg!(windows) {
        "It works whichever program has focus; one another program already uses can't be taken."
    } else if hypr {
        "Hyprland binds shortcuts in its config: add the line below to it (Copy; hyprland.lua or hyprland.conf), then save the action."
    } else {
        "When the action is saved, the desktop asks you to allow the shortcut (no root needed)."
    };
    let mut note = Frame::default()
        .with_label(how)
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    note.set_label_size(12);
    col.fixed(&note, 34);
    let mut line = Frame::default().with_align(Align::Left | Align::Inside | Align::Clip);
    line.set_label_font(Font::Courier);
    line.set_label_size(12);
    if hypr {
        col.fixed(&line, ROW);
    } else {
        line.hide();
    }
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut copy = button(&mut bar, "Copy line", 100);
    if !hypr {
        copy.hide();
    }
    Frame::default();
    let mut ok = button(&mut bar, "OK", 90);
    let mut cancel = button(&mut bar, "Cancel", 90);
    bar.end();
    col.fixed(&bar, ROW);
    col.end();
    win.end();
    // Buttons don't take the keys: every key goes to the window.
    for b in [&mut copy, &mut ok, &mut cancel] {
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
    copy.set_callback({
        let got = got.clone();
        move |_| {
            if let Some(l) = got
                .get()
                .and_then(|(k, m)| hotkeys::hyprland_bind(k, m, hotkeys::hyprland_lua()))
            {
                app::copy(&l);
            }
        }
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
                    if let Some(l) = hotkeys::hyprland_bind(key.code, mask, hotkeys::hyprland_lua()) {
                        line.set_label(&l.replace('@', "@@"));
                    }
                    ok.activate();
                }
                None => shown.set_label("That key can't be used; try another"),
            }
            true
        }
    });
    run(&mut win, &answer, || true).then(|| got.get()).flatten()
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
    let r = row("Name");
    let mut n = Input::default();
    n.set_value(name);
    r.end();
    col.fixed(&r, ROW);
    let r = row("Value");
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
    let ok = run(&mut win, &answer, || !n.value().trim().is_empty());
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
    super::header(&mut col, &[200], &["Name", "Value"]);
    let mut list = super::rows::Rows::new(if edit {
        "Nothing yet. Add one below."
    } else {
        "Nothing."
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
    let labels: &[&str] = if edit { &["Add", "Edit", "Delete"] } else { &[] };
    let b = if edit {
        super::grid(&mut col, labels, 1)
    } else {
        Vec::new()
    };
    list.set_menu(&b);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    let mut json = button(&mut bar, if edit { "Edit JSON" } else { "View JSON" }, 120);
    if edit {
        ok_cancel(&mut bar, &answer, false);
    } else {
        Frame::default();
        let mut close = button(&mut bar, "Close", 90);
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
            let title = if old.is_some() { "Change" } else { "Add" };
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
                    Err("An object, please.".into())
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
