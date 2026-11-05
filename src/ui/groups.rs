//! The settings of an action group. Its creator declares typed settings (a time, a number, a
//! switch...), the files that go with it and the commands and timers it uses; whoever has the group
//! sets the values. Actions read a setting as `%group.name%`. Export and import carry all of it.
use super::{dialogs, rows::Rows, ROW};
use crate::groups::{self, Def, File, Kind, Meta};
use crate::i18n::{tr, trf};
use crate::model::Obj;
use crate::Config;
use fltk::browser::CheckBrowser;
use fltk::enums::Align;
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::{app, dialog};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::sync::watch;

const MAX_FILE: u64 = 2 * 1024 * 1024;

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Text => tr("group.kind.text"),
        Kind::Integer => tr("group.kind.integer"),
        Kind::Number => tr("group.kind.number"),
        Kind::Boolean => tr("group.kind.boolean"),
        Kind::Duration => tr("group.kind.duration"),
        Kind::Choice => tr("group.kind.choice"),
    }
}

fn kind_hint(k: Kind) -> &'static str {
    match k {
        Kind::Text => tr("group.hint.text"),
        Kind::Integer => tr("group.hint.integer"),
        Kind::Number => tr("group.hint.number"),
        Kind::Boolean => tr("group.hint.boolean"),
        Kind::Duration => tr("group.hint.duration"),
        Kind::Choice => tr("group.hint.choice"),
    }
}

/// Ask for a value of `def`'s kind until it reads as one, or the user gives up.
fn ask_value(def: &Def, current: &Value) -> Option<Value> {
    if def.kind == Kind::Boolean {
        let on = dialogs::choice(
            &trf("group.pick_boolean", &[("name", &def.label)]),
            tr("group.false"),
            tr("group.true"),
        );
        return Some(Value::Bool(on));
    }
    if def.kind == Kind::Choice {
        let options: Vec<&str> = def.options.iter().map(String::as_str).collect();
        let title = if def.description.is_empty() {
            def.label.clone()
        } else {
            format!("{}\n{}", def.label, def.description)
        };
        return dialogs::pick_one(&title, &options).map(|i| Value::String(options[i].to_string()));
    }
    let mut text = def.kind.show(current);
    loop {
        let prompt = if def.description.is_empty() {
            kind_hint(def.kind).to_string()
        } else {
            format!("{}\n{}", def.description, kind_hint(def.kind))
        };
        text = super::ask(&def.label, &prompt, &text)?;
        match def.parse(&text) {
            Ok(v) => return Some(v),
            Err(_) => dialogs::alert(&trf("group.invalid", &[("hint", kind_hint(def.kind))])),
        }
    }
}

/// A new or changed setting. `taken`: the names of the other settings.
fn edit_def(old: Option<&Def>, taken: &[String]) -> Option<Def> {
    let (mut win, answer) = dialogs::window(
        480,
        285,
        tr(if old.is_some() {
            "group.edit_setting"
        } else {
            "group.add_setting"
        }),
    );
    let mut col = Flex::new(10, 10, 460, 265, None).column();
    col.set_pad(4);
    let field = |col: &mut Flex, label: &str, value: &str| {
        let r = dialogs::row(label);
        let mut i = Input::default();
        i.set_value(value);
        r.end();
        col.fixed(&r, ROW);
        i
    };
    let (n0, l0, d0, s0) = old.map_or(("", "", String::new(), ""), |d| {
        (
            d.name.as_str(),
            d.label.as_str(),
            d.kind.show(&d.default),
            d.description.as_str(),
        )
    });
    let mut name = field(&mut col, tr("group.name"), n0);
    let label = field(&mut col, tr("group.label"), l0);
    let r = dialogs::row(tr("group.type"));
    let mut kind = super::fit::choice();
    for k in groups::KINDS {
        kind.add_choice(kind_label(*k));
    }
    kind.set_value(
        old.and_then(|d| groups::KINDS.iter().position(|k| *k == d.kind))
            .unwrap_or(0) as i32,
    );
    r.end();
    col.fixed(&r, ROW);
    let options = field(
        &mut col,
        tr("group.options"),
        &old.map(|d| d.options.join(", ")).unwrap_or_default(),
    );
    let default = field(&mut col, tr("group.default"), &d0);
    let description = field(&mut col, tr("group.description"), s0);
    let mut hint = Frame::default()
        .with_label(tr("group.setting_hint"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    hint.set_label_color(super::muted());
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    dialogs::ok_cancel(&mut bar, &answer, false);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    let _ = name.take_focus();
    let result: Rc<RefCell<Option<Def>>> = Rc::default();
    let ok = dialogs::run(&mut win, &answer, || {
        let k = groups::KINDS[kind.value().max(0) as usize];
        let n = name.value().trim().to_string();
        let problem = if !groups::valid_variable_name(&n) {
            Some(tr("group.bad_name"))
        } else if taken.contains(&n) {
            Some(tr("group.name_taken"))
        } else if label.value().trim().is_empty() {
            Some(tr("group.needs_label"))
        } else {
            None
        };
        if let Some(p) = problem {
            dialogs::alert(p);
            return false;
        }
        let mut def = Def {
            name: n,
            label: label.value().trim().into(),
            kind: k,
            description: description.value().trim().into(),
            options: if k == Kind::Choice {
                options
                    .value()
                    .split(',')
                    .map(|o| o.trim().to_string())
                    .filter(|o| !o.is_empty())
                    .collect()
            } else {
                Vec::new()
            },
            ..Default::default()
        };
        if k == Kind::Choice && def.options.is_empty() {
            dialogs::alert(tr("group.needs_options"));
            return false;
        }
        match def.parse(&default.value()) {
            Ok(v) => {
                def.default = v;
                *result.borrow_mut() = Some(def);
                true
            }
            Err(_) => {
                dialogs::alert(&trf("group.invalid", &[("hint", kind_hint(k))]));
                false
            }
        }
    });
    app::delete_widget(win);
    ok.then(|| result.borrow_mut().take()).flatten()
}

/// The files of a group: add, remove, and save them somewhere to use them.
fn files_dialog(cfg: &Config, group: &str, files: &mut Vec<File>) {
    let working = Rc::new(RefCell::new(files.clone()));
    let (mut win, answer) = dialogs::window(520, 360, tr("group.files"));
    let mut col = Flex::new(10, 10, 500, 340, None).column();
    col.set_pad(6);
    let mut hint = Frame::default()
        .with_label(tr("group.files_hint"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    hint.set_label_color(super::muted());
    col.fixed(&hint, 2 * ROW);
    super::header(&mut col, &[300], &[tr("group.file"), tr("group.size")]);
    let mut list = Rows::new(tr("group.no_files"));
    list.set_column_widths(&[300]);
    let fill = {
        let (working, list) = (working.clone(), list.clone());
        move || {
            let mut list = list.clone();
            let keep = list.value();
            list.clear();
            for f in working.borrow().iter() {
                let bytes = f.content.len() / 4 * 3;
                list.add(&format!("{}\t{} KB", f.name, bytes.div_ceil(1024)));
            }
            list.select(keep);
        }
    };
    fill();
    let b = super::grid(
        &mut col,
        &[
            tr("btn.add_file"),
            tr("btn.delete"),
            tr("btn.save_files"),
            tr("btn.copy_url"),
            tr("btn.save_page"),
        ],
        1,
    );
    list.set_menu(&b);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    dialogs::ok_cancel(&mut bar, &answer, false);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    let mut b = b.into_iter();
    let (mut add, mut del, mut save, mut copy, mut page) = (
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
    );
    // The selected .html file: an overlay.
    let (base, values) = (crate::overlays::base(cfg), Value::Object(groups::values(cfg, group)));
    let overlay = {
        let (working, list) = (working.clone(), list.clone());
        move || {
            let i = usize::try_from(list.value() - 1).ok()?;
            let f = working.borrow().get(i).cloned()?;
            if f.name.to_ascii_lowercase().ends_with(".html") {
                Some(f)
            } else {
                dialogs::alert(tr("group.not_overlay"));
                None
            }
        }
    };
    copy.set_callback({
        let (base, group, overlay) = (base.clone(), group.to_string(), overlay.clone());
        move |_| {
            if let Some(f) = overlay() {
                app::copy(&crate::overlays::url_at(&base, &group, &f.name));
                dialogs::message(tr("group.url_copied"));
            }
        }
    });
    page.set_callback({
        let (base, group, overlay) = (base, group.to_string(), overlay);
        move |_| {
            let Some(f) = overlay() else { return };
            let mut chooser = dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseDir);
            chooser.show();
            let folder = chooser.filename();
            if folder.as_os_str().is_empty() {
                return;
            }
            let Some(html) = crate::overlays::standalone_at(&base, &values, &group, &f) else {
                return dialogs::alert(tr("group.page_failed"));
            };
            let page = groups::text_file(&f.name, &html);
            match groups::save_files(&[page], &folder) {
                Ok(_) => dialogs::message(&trf("group.page_saved", &[("folder", &folder.display().to_string())])),
                Err(e) => dialogs::alert(&e),
            }
        }
    });
    add.set_callback({
        let (working, fill) = (working.clone(), fill.clone());
        move |_| {
            let mut chooser = dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseFile);
            chooser.show();
            let path = chooser.filename();
            if path.as_os_str().is_empty() {
                return;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if !groups::valid_file_name(&name) {
                return dialogs::alert(&trf("group.bad_file_name", &[("name", &name)]));
            }
            if working.borrow().iter().any(|f| f.name.eq_ignore_ascii_case(&name)) {
                return dialogs::alert(&trf("group.file_exists", &[("name", &name)]));
            }
            if std::fs::metadata(&path).map_or(true, |m| m.len() > MAX_FILE) {
                return dialogs::alert(tr("group.file_too_big"));
            }
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let content = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
                    working.borrow_mut().push(File { name, content });
                    fill();
                }
                Err(e) => dialogs::alert(&e.to_string()),
            }
        }
    });
    del.set_callback({
        let (working, fill, list) = (working.clone(), fill.clone(), list.clone());
        move |_| {
            if let Ok(i) = usize::try_from(list.value() - 1) {
                if i < working.borrow().len() {
                    working.borrow_mut().remove(i);
                    fill();
                }
            }
        }
    });
    save.set_callback({
        let working = working.clone();
        move |_| {
            let mut chooser = dialog::NativeFileChooser::new(dialog::NativeFileChooserType::BrowseDir);
            chooser.show();
            let folder = chooser.filename();
            if folder.as_os_str().is_empty() {
                return;
            }
            match groups::save_files(&working.borrow(), &folder) {
                Ok(saved) => dialogs::message(&trf(
                    "group.files_saved",
                    &[
                        ("count", &saved.len().to_string()),
                        ("folder", &folder.display().to_string()),
                    ],
                )),
                Err(e) => dialogs::alert(&e),
            }
        }
    });
    if dialogs::run(&mut win, &answer, || true) {
        *files = working.borrow().clone();
    }
    app::delete_widget(win);
}

/// Tick the commands and timers the group uses. Those that name the group as theirs stay ticked.
fn members_dialog(c: &Config, group: &str, meta: &mut Meta) {
    let id = |o: &Obj| o.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let own = |o: &Obj| o.get("group").and_then(Value::as_str) == Some(group);
    let name = |o: &Obj, key: &str| {
        o.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| o.get("name").and_then(Value::as_str))
            .unwrap_or("")
            .to_string()
    };
    let (mut win, answer) = dialogs::window(520, 420, tr("group.members"));
    let mut col = Flex::new(10, 10, 500, 400, None).column();
    col.set_pad(6);
    let mut hint = Frame::default()
        .with_label(tr("group.members_hint"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    hint.set_label_color(super::muted());
    col.fixed(&hint, 2 * ROW);
    let mut list = CheckBrowser::default();
    super::scroll::view(&mut list);
    let mut ids = Vec::new();
    for o in &c.commands {
        list.add(
            &format!("{}: {}", tr("group.command"), name(o, "command")),
            own(o) || meta.commands.contains(&id(o)),
        );
        ids.push((true, id(o), own(o)));
    }
    for o in &c.timers {
        list.add(
            &format!("{}: {}", tr("group.timer"), name(o, "name")),
            own(o) || meta.timers.contains(&id(o)),
        );
        ids.push((false, id(o), own(o)));
    }
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    dialogs::ok_cancel(&mut bar, &answer, false);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    if dialogs::run(&mut win, &answer, || true) {
        meta.commands.clear();
        meta.timers.clear();
        for (i, (command, id, own)) in ids.iter().enumerate() {
            if list.checked(i as i32 + 1) && !own {
                if *command { &mut meta.commands } else { &mut meta.timers }.push(id.clone());
            }
        }
    }
    app::delete_widget(win);
}

/// The group's settings: change values, and declare settings, files and members.
pub fn settings(cfg: &Arc<watch::Sender<Config>>, group: &str) {
    let (meta0, values0) = {
        let c = cfg.borrow();
        (groups::meta(&c, group), groups::values(&c, group))
    };
    let state = Rc::new(RefCell::new((meta0, values0)));
    let (mut win, answer) = dialogs::window(680, 470, &trf("group.title", &[("name", group)]));
    let mut col = Flex::new(10, 10, 660, 450, None).column();
    col.set_pad(6);
    let mut hint = Frame::default()
        .with_label(tr("group.hint"))
        .with_align(Align::Left | Align::Inside | Align::Wrap);
    hint.set_label_color(super::muted());
    col.fixed(&hint, 2 * ROW);
    let r = dialogs::row(tr("group.description"));
    let mut description = Input::default();
    description.set_value(&state.borrow().0.description);
    // Show a long description from its start, not scrolled to its end.
    let _ = description.set_position(0);
    let _ = description.set_mark(0);
    r.end();
    col.fixed(&r, ROW);
    super::header(
        &mut col,
        &[250, 190],
        &[tr("group.setting"), tr("dialogs.value"), tr("group.type")],
    );
    let mut list = Rows::new(tr("group.no_settings"));
    list.set_column_widths(&[250, 190]);
    let fill = {
        let (state, list) = (state.clone(), list.clone());
        move || {
            let mut list = list.clone();
            let keep = list.value();
            list.clear();
            let (meta, values) = &*state.borrow();
            for d in &meta.variables {
                let v = values.get(&d.name).unwrap_or(&d.default);
                list.add(&format!(
                    "{}\t{}\t{}",
                    d.label.replace('\t', " "),
                    d.kind.show(v).replace(['\t', '\n'], " "),
                    kind_label(d.kind)
                ));
            }
            list.select(keep);
        }
    };
    fill();
    let b = super::grid(
        &mut col,
        &[
            tr("btn.change_value"),
            tr("btn.add_setting"),
            tr("btn.edit_setting"),
            tr("btn.delete_setting"),
            tr("btn.group_files"),
            tr("btn.group_members"),
        ],
        1,
    );
    list.set_menu(&b);
    let mut bar = Flex::default().row();
    bar.set_pad(8);
    dialogs::ok_cancel(&mut bar, &answer, false);
    bar.end();
    col.fixed(&bar, ROW + 4);
    col.end();
    win.end();
    win.resizable(&col);

    let picked = {
        let (state, list) = (state.clone(), list.clone());
        move || {
            usize::try_from(list.value() - 1)
                .ok()
                .filter(|i| *i < state.borrow().0.variables.len())
        }
    };
    let change = {
        let (state, fill, picked) = (state.clone(), fill.clone(), picked.clone());
        move || {
            let Some(i) = picked() else { return };
            let def = state.borrow().0.variables[i].clone();
            let now = state
                .borrow()
                .1
                .get(&def.name)
                .cloned()
                .unwrap_or_else(|| def.default.clone());
            if let Some(v) = ask_value(&def, &now) {
                state.borrow_mut().1.insert(def.name, v);
                fill();
            }
        }
    };
    let mut b = b.into_iter();
    let (mut ch, mut add, mut edit, mut del, mut files, mut members) = (
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
        b.next().unwrap(),
    );
    ch.set_callback({
        let change = change.clone();
        move |_| change()
    });
    list.set_callback({
        let change = change.clone();
        move |_| {
            if app::event_clicks() {
                change()
            }
        }
    });
    add.set_callback({
        let (state, fill) = (state.clone(), fill.clone());
        move |_| {
            let taken: Vec<String> = state.borrow().0.variables.iter().map(|d| d.name.clone()).collect();
            if let Some(d) = edit_def(None, &taken) {
                let mut s = state.borrow_mut();
                s.1.insert(d.name.clone(), d.default.clone());
                s.0.variables.push(d);
                drop(s);
                fill();
            }
        }
    });
    edit.set_callback({
        let (state, fill, picked) = (state.clone(), fill.clone(), picked.clone());
        move |_| {
            let Some(i) = picked() else { return };
            let old = state.borrow().0.variables[i].clone();
            let taken: Vec<String> = state
                .borrow()
                .0
                .variables
                .iter()
                .filter(|d| d.name != old.name)
                .map(|d| d.name.clone())
                .collect();
            if let Some(d) = edit_def(Some(&old), &taken) {
                let mut s = state.borrow_mut();
                let kept = s.1.remove(&old.name).filter(|v| d.kind.accepts(v));
                s.1.insert(d.name.clone(), kept.unwrap_or_else(|| d.default.clone()));
                s.0.variables[i] = d;
                drop(s);
                fill();
            }
        }
    });
    del.set_callback({
        let (state, fill, picked) = (state.clone(), fill.clone(), picked.clone());
        move |_| {
            let Some(i) = picked() else { return };
            let mut s = state.borrow_mut();
            let d = s.0.variables.remove(i);
            s.1.remove(&d.name);
            drop(s);
            fill();
        }
    });
    files.set_callback({
        let (state, cfg, group) = (state.clone(), cfg.clone(), group.to_string());
        move |_| {
            let mut f = state.borrow().0.files.clone();
            files_dialog(&cfg.borrow(), &group, &mut f);
            state.borrow_mut().0.files = f;
        }
    });
    members.set_callback({
        let (state, cfg, group) = (state.clone(), cfg.clone(), group.to_string());
        move |_| {
            let mut m = state.borrow().0.clone();
            members_dialog(&cfg.borrow(), &group, &mut m);
            state.borrow_mut().0 = m;
        }
    });
    let ok = dialogs::run(&mut win, &answer, || {
        let mut s = state.borrow_mut();
        s.0.description = description.value().trim().into();
        match s.0.validate() {
            Ok(()) => true,
            Err(e) => {
                dialogs::alert(&e);
                false
            }
        }
    });
    app::delete_widget(win);
    if ok {
        let (meta, values) = state.borrow().clone();
        cfg.send_modify(|c| {
            if let Err(e) = groups::set_meta(c, group, meta).and_then(|_| groups::set_values(c, group, values)) {
                crate::warn(format!("Group {group}: {e}"));
            }
        });
        crate::save_later(&cfg.borrow());
    }
}
