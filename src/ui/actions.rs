//! The Actions tab, as in Streamer.bot: actions by group on the left; the selected action's
//! triggers and sub-actions on the right. Drag an action onto a group (or another action) to move it
//! there; drag a sub-action to reorder it or into a container (group, branch, loop).

use super::forms::{self, Done, Lists};
use super::{button, dialogs, ROW};
use crate::edit::{self, Path};
use crate::engine::{UiCmd, DEFAULT_QUEUE};
use crate::model::Obj;
use crate::types::{name, SUB_ACTION_INFO, TRIGGERS};
use crate::Config;
use fltk::browser::HoldBrowser;
use fltk::button::Button;
use fltk::enums::{Align, Color, Cursor, Event, Font};
use fltk::frame::Frame;
use fltk::group::{Flex, Tabs};
use fltk::prelude::*;
use fltk::tree::{Tree, TreeItem, TreeReason, TreeSelect};
use fltk::{app, dialog, draw};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, watch};

pub struct Actions {
    cfg: Arc<watch::Sender<Config>>,
    cmds: UnboundedSender<UiCmd>,
    tree: Tree,
    triggers: HoldBrowser,
    subs: Tree,
    title: Frame,
    /// The selected action's id.
    selected: Option<String>,
    /// Tree items and what they are: an action id, or a group name.
    items: Vec<(TreeItem, Node)>,
    sub_items: Vec<(TreeItem, Path)>,
}

#[derive(Clone, PartialEq)]
enum Node {
    Action(String),
    Group(String),
}

fn s<'a>(o: &'a Obj, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

/// FLTK draws "@" as a symbol marker; "@@" is a plain "@".
fn plain(t: &str) -> String {
    t.replace('@', "@@")
}

fn short(t: &str) -> String {
    let t = t.replace(['\r', '\n'], " ");
    if t.chars().count() > 60 {
        format!("{}...", t.chars().take(60).collect::<String>())
    } else {
        t
    }
}

/// Names by id (actions, queues, timers, commands, user groups, WebSocket clients and servers, OBS
/// and Streamlabs connections), so labels show names rather than ids.
pub type Names = std::collections::HashMap<String, String>;

pub fn names(c: &Config) -> Names {
    let objs = [
        &c.actions.queues,
        &c.timers,
        &c.commands,
        &c.groups,
        &c.websockets.connections,
        &c.websockets.servers,
        &c.obs.connections,
        &c.slobs.connections,
    ];
    let mut out: Names = objs
        .into_iter()
        .flatten()
        .map(|o| (s(o, "id").to_string(), s(o, "name").to_string()))
        .collect();
    out.extend(c.actions.actions.iter().map(|a| (a.id.clone(), a.name.clone())));
    out.insert(DEFAULT_QUEUE.into(), "Default".into());
    out
}

/// How a sub-action reads in the list.
pub fn sub_label(o: &Obj, names: &Names) -> String {
    let k = edit::kind(o);
    let what = SUB_ACTION_INFO.binary_search_by_key(&(k as u32), |x| x.0).map_or_else(
        |_| {
            name(crate::types::SUB_ACTIONS, k as u32)
                .unwrap_or("Unknown")
                .to_string()
        },
        |i| SUB_ACTION_INFO[i].1.to_string(),
    );
    let v = |k: &str| o.get(k).map(crate::vars::text).unwrap_or_default();
    let detail = match k {
        99901 => return "True".into(),
        99902 => return "False".into(),
        99904 => return "Default".into(),
        99903 => {
            let vals: Vec<String> = o
                .get("values")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(crate::vars::text).collect())
                .unwrap_or_default();
            return format!("Case: {}", short(&vals.join(", ")));
        }
        99900 => format!(
            "{}{}",
            v("name"),
            if o.get("random").and_then(Value::as_bool) == Some(true) {
                " (random)"
            } else {
                ""
            }
        ),
        120 | 128 => {
            let op = o.get("operation").and_then(Value::as_u64).unwrap_or(0) as usize;
            let op = if k == 120 {
                forms::OPS.get(op)
            } else {
                forms::WHILE_OPS.get(op)
            }
            .copied()
            .unwrap_or("?");
            format!("{} {op} {}", v("input"), v("value"))
        }
        123 | 122 => format!("{} = {}", v("variableName"), v("value")),
        121 => format!("{} -> {}", v("variableName"), v("destinationVariable")),
        1002 => format!("{} ms", v("value")),
        1009 => v("value"),
        1020 => v("message"),
        _ => o
            .iter()
            .filter(|(k, _)| {
                !["id", "weight", "type", "parentId", "enabled", "index", "subActions"].contains(&k.as_str())
            })
            .find_map(|(_, x)| x.as_str().filter(|t| !t.is_empty()))
            .map(|t| names.get(t).map_or(t, |n| n.as_str()).to_string())
            .unwrap_or_default(),
    };
    let mut out = if detail.is_empty() {
        what
    } else {
        format!("{what}: {}", short(&detail))
    };
    if !crate::engine::supported(k as u32) {
        out += " (not supported)";
    }
    out
}

pub fn trigger_label(t: &Obj, names: &Names) -> String {
    let k = t.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
    let what = name(TRIGGERS, k).unwrap_or("Unknown");
    let detail = t
        .iter()
        .filter(|(k, _)| !["id", "type", "enabled", "exclusions"].contains(&k.as_str()))
        .filter_map(|(k, x)| {
            let x = crate::vars::text(x);
            let x = names.get(&x).cloned().unwrap_or(x);
            (!x.is_empty()).then(|| format!("{}: {x}", k.trim_end_matches("Id")))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let on = t.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let mut out = if detail.is_empty() {
        what.to_string()
    } else {
        format!("{what} ({})", short(&detail))
    };
    if !on {
        out += " [disabled]";
    }
    out
}

impl Actions {
    /// Change the config, save it, and redraw.
    fn edit(&mut self, f: impl FnOnce(&mut Config)) {
        self.cfg.send_modify(f);
        let _ = crate::save(&self.cfg.borrow());
        self.refresh();
    }

    /// Change the selected action.
    fn edit_action(&mut self, f: impl FnOnce(&mut crate::model::Action)) {
        let Some(id) = self.selected.clone() else { return };
        self.edit(|c| {
            if let Some(a) = c.actions.actions.iter_mut().find(|a| a.id == id) {
                f(a);
            }
        });
    }

    fn node(&self, item: &TreeItem) -> Option<Node> {
        self.items.iter().find(|(i, _)| i == item).map(|(_, n)| n.clone())
    }

    fn sub_path(&self, item: &TreeItem) -> Option<Path> {
        self.sub_items.iter().find(|(i, _)| i == item).map(|(_, p)| p.clone())
    }

    fn selected_sub(&self) -> Option<Path> {
        self.subs.first_selected_item().and_then(|i| self.sub_path(&i))
    }

    /// The group of what's selected on the left ("" for none).
    fn current_group(&self) -> String {
        let Some(item) = self.tree.first_selected_item() else {
            return String::new();
        };
        match self.node(&item) {
            Some(Node::Group(g)) => g,
            Some(Node::Action(id)) => self
                .cfg
                .borrow()
                .actions
                .actions
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.group.clone())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    /// Rebuild everything from the config, keeping the selection and open groups.
    pub fn refresh(&mut self) {
        let c = self.cfg.borrow().clone_actions();
        // Actions by group, groups first, both by name.
        let closed: Vec<String> = self
            .items
            .iter()
            .filter_map(|(i, n)| match n {
                Node::Group(g) if !i.is_open() => Some(g.clone()),
                _ => None,
            })
            .collect();
        self.items.clear();
        let Some(root) = self.tree.root() else { return };
        // Not clear(): that deletes the root too.
        self.tree.clear_children(&root);
        let mut groups: Vec<String> = c.iter().map(|a| a.group.clone()).filter(|g| !g.is_empty()).collect();
        groups.extend(
            self.cfg
                .borrow()
                .actions
                .groups
                .iter()
                .filter_map(|g| g.as_str().map(String::from)),
        );
        groups.sort_by_key(|g| g.to_lowercase());
        groups.dedup();
        let mut keep = None;
        let mut add =
            |tree: &mut Tree, parent: &TreeItem, a: &crate::model::Action, items: &mut Vec<(TreeItem, Node)>| {
                if let Some(mut i) = tree.insert(parent, &plain(&a.name), parent.children()) {
                    if !a.enabled {
                        i.set_label_fgcolor(Color::from_rgb(130, 130, 130));
                    }
                    if self.selected.as_deref() == Some(&a.id) {
                        keep = Some(i.clone());
                    }
                    items.push((i, Node::Action(a.id.clone())));
                }
            };
        let mut sorted = c.clone();
        sorted.sort_by_key(|a| a.name.to_lowercase());
        for g in &groups {
            let Some(mut gi) = self.tree.insert(&root, &plain(g), root.children()) else {
                continue;
            };
            gi.set_label_font(Font::HelveticaBold);
            self.items.push((gi.clone(), Node::Group(g.clone())));
            for a in sorted.iter().filter(|a| &a.group == g) {
                add(&mut self.tree, &gi, a, &mut self.items);
            }
            if closed.contains(g) {
                gi.close();
            }
        }
        for a in sorted.iter().filter(|a| a.group.is_empty()) {
            add(&mut self.tree, &root, a, &mut self.items);
        }
        match keep {
            Some(i) => {
                let _ = self.tree.select_only(&i, false);
            }
            None => self.selected = None,
        }
        self.tree.redraw();
        self.refresh_action();
    }

    /// The right side, for the selected action.
    fn refresh_action(&mut self) {
        let a = self
            .selected
            .as_ref()
            .and_then(|id| self.cfg.borrow().actions.actions.iter().find(|a| &a.id == id).cloned());
        let keep = self.selected_sub();
        let keep_trigger = self.triggers.value();
        self.triggers.clear();
        if let Some(root) = self.subs.root() {
            self.subs.clear_children(&root);
        }
        self.sub_items.clear();
        let Some(a) = a else {
            self.title.set_label("Select an action, or add one.");
            self.subs.redraw();
            return;
        };
        let mut flags = Vec::new();
        for (on, what) in [
            (!a.enabled, "disabled"),
            (a.random_action, "random"),
            (a.concurrent, "concurrent"),
        ] {
            if on {
                flags.push(what);
            }
        }
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!(" ({})", flags.join(", "))
        };
        self.title.set_label(&plain(&format!("{}{flags}", a.name)));
        let names = names(&self.cfg.borrow());
        for t in &a.triggers {
            self.triggers.add(&format!("@.{}", trigger_label(t, &names)));
        }
        if keep_trigger > 0 && keep_trigger <= self.triggers.size() {
            self.triggers.select(keep_trigger);
        }
        let Some(root) = self.subs.root() else { return };
        fn add(
            t: &mut Tree,
            parent: &TreeItem,
            list: &[Obj],
            path: &mut Path,
            items: &mut Vec<(TreeItem, Path)>,
            names: &Names,
        ) {
            for (i, o) in list.iter().enumerate() {
                path.push(i);
                if let Some(mut it) = t.insert(parent, &plain(&sub_label(o, names)), parent.children()) {
                    let k = edit::kind(o);
                    if o.get("enabled").and_then(Value::as_bool) == Some(false) || !crate::engine::supported(k as u32) {
                        it.set_label_fgcolor(Color::from_rgb(130, 130, 130));
                    }
                    if (99901..=99904).contains(&k) {
                        it.set_label_font(Font::HelveticaItalic);
                    }
                    items.push((it.clone(), path.clone()));
                    let kids: Vec<Obj> = o
                        .get("subActions")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect())
                        .unwrap_or_default();
                    add(t, &it, &kids, path, items, names);
                }
                path.pop();
            }
        }
        add(
            &mut self.subs,
            &root,
            &a.sub_actions,
            &mut Vec::new(),
            &mut self.sub_items,
            &names,
        );
        if let Some(p) = keep {
            if let Some((i, _)) = self.sub_items.iter().find(|(_, x)| *x == p) {
                let _ = self.subs.select_only(i, false);
            }
        }
        self.subs.redraw();
    }
}

impl Config {
    fn clone_actions(&self) -> Vec<crate::model::Action> {
        self.actions.actions.clone()
    }
}

fn bar(col: &mut Flex, labels: &[&str]) -> Vec<Button> {
    let mut row = Flex::default().row();
    row.set_pad(4);
    let out = labels
        .iter()
        .map(|l| button(&mut row, l, (l.len() as i32 * 8 + 24).max(60)))
        .collect();
    Frame::default();
    row.end();
    col.fixed(&row, ROW + 2);
    out
}

fn tree() -> Tree {
    let mut t = Tree::default();
    t.set_show_root(false);
    t.set_select_mode(TreeSelect::Single);
    t
}

/// Ask for an action name.
fn ask_name(title: &str, current: &str) -> Option<String> {
    super::ask(title, "Name:", current)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn tab(tabs: &Tabs, cfg: Arc<watch::Sender<Config>>, cmds: UnboundedSender<UiCmd>) -> Rc<RefCell<Actions>> {
    let mut row = Flex::new(tabs.x(), tabs.y() + 25, tabs.w(), tabs.h() - 25, None)
        .row()
        .with_label("Actions");
    row.set_margin(8);
    row.set_pad(8);
    let mut left = Flex::default().column();
    left.set_pad(4);
    let t = Frame::default()
        .with_label("Actions")
        .with_align(Align::Left | Align::Inside);
    left.fixed(&t, ROW - 4);
    let actions_tree = tree();
    let lb = bar(&mut left, &["Add", "Edit", "Delete", "Test"]);
    let lb2 = bar(&mut left, &["Import", "Export"]);
    left.end();
    row.fixed(&left, 300);
    let mut right = Flex::default().column();
    right.set_pad(4);
    let mut title = Frame::default().with_align(Align::Left | Align::Inside);
    title.set_label_font(Font::HelveticaBold);
    right.fixed(&title, ROW - 2);
    let t = Frame::default()
        .with_label("Triggers")
        .with_align(Align::Left | Align::Inside);
    right.fixed(&t, ROW - 4);
    let triggers = HoldBrowser::default();
    right.fixed(&triggers, 120);
    let tb = bar(&mut right, &["Add", "Edit", "On/Off", "Test...", "Delete"]);
    let t = Frame::default()
        .with_label("Sub-actions")
        .with_align(Align::Left | Align::Inside);
    right.fixed(&t, ROW - 4);
    let subs = tree();
    let sb = bar(&mut right, &["Add", "Edit", "On/Off", "Delete"]);
    right.end();
    row.end();

    let me = Rc::new(RefCell::new(Actions {
        cfg,
        cmds,
        tree: actions_tree.clone(),
        triggers: triggers.clone(),
        subs: subs.clone(),
        title,
        selected: None,
        items: Vec::new(),
        sub_items: Vec::new(),
    }));
    let on = |b: &Button, f: Box<dyn Fn(&mut Actions)>| {
        let me = me.clone();
        let mut b = b.clone();
        b.set_callback(move |_| f(&mut me.borrow_mut()));
    };

    // Left: the actions.
    let mut t = actions_tree.clone();
    t.set_callback({
        let me = me.clone();
        move |t| {
            let Some(item) = t.callback_item() else { return };
            if t.callback_reason() != TreeReason::Selected {
                return;
            }
            let Ok(mut m) = me.try_borrow_mut() else { return };
            if let Some(Node::Action(id)) = m.node(&item) {
                m.selected = Some(id);
                m.refresh_action();
            }
        }
    });
    on(
        &lb[0],
        Box::new(|m| {
            let Some(name) = ask_name("New action", "") else { return };
            let a = edit::new_action(&name, &m.current_group());
            m.selected = Some(a.id.clone());
            m.edit(|c| c.actions.actions.push(a));
        }),
    );
    {
        let me2 = me.clone();
        let mut b = lb[1].clone();
        b.set_callback(move |_| edit_action(&me2));
    }
    on(
        &lb[2],
        Box::new(|m| {
            let Some(id) = m.selected.clone() else { return };
            let name = m
                .cfg
                .borrow()
                .actions
                .actions
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            if dialog::choice2_default(&format!("Delete the action {}?", plain(&name)), "Cancel", "Delete", "")
                == Some(1)
            {
                m.selected = None;
                m.edit(|c| c.actions.actions.retain(|a| a.id != id));
            }
        }),
    );
    on(
        &lb[3],
        Box::new(|m| {
            if let Some(id) = m.selected.clone() {
                super::tell(&m.cmds, move |e| drop(e.test(&id)));
            }
        }),
    );
    on(
        &lb2[0],
        Box::new(|m| {
            let Some(x) = dialogs::import() else { return };
            let q = format!(
                "Import this?\n\n{}\n\nItems with the same name are replaced.",
                edit::summary(&x)
            );
            if dialog::choice2_default(&plain(&q), "Cancel", "Import", "") != Some(1) {
                return;
            }
            let mut result = Ok(Vec::new());
            m.edit(|c| result = edit::import(c, &x));
            match result {
                Ok(u) if u.is_empty() => crate::say("imported".into()),
                Ok(u) => {
                    let msg = format!(
                        "Imported. These sub-actions are kept but won't run here: {}",
                        u.join(", ")
                    );
                    crate::say(msg.clone());
                    dialog::message_default(&plain(&msg));
                }
                Err(e) => dialog::alert_default(&plain(&e)),
            }
        }),
    );
    on(
        &lb2[1],
        Box::new(|m| {
            let list: Vec<(String, String)> = {
                let c = m.cfg.borrow();
                let mut l: Vec<(String, String)> = c
                    .actions
                    .actions
                    .iter()
                    .map(|a| (a.id.clone(), a.name.clone()))
                    .collect();
                l.sort_by_key(|(_, n)| n.to_lowercase());
                l
            };
            let cfg = m.cfg.clone();
            dialogs::export(&list, m.selected.as_deref(), move |ids, meta| {
                crate::model::write_export(&edit::export(&cfg.borrow(), ids, meta))
            });
        }),
    );

    // Drag an action onto a group, or onto an action (its group), or to the bottom (no group).
    let me2 = me.clone();
    drag(
        &actions_tree,
        {
            let me = me.clone();
            move |from, to| {
                let mut m = me.borrow_mut();
                let Some(Node::Action(id)) = m.node(&from) else { return };
                let group = match to.as_ref().and_then(|t| m.node(t)) {
                    Some(Node::Group(g)) => g,
                    Some(Node::Action(other)) => m
                        .cfg
                        .borrow()
                        .actions
                        .actions
                        .iter()
                        .find(|a| a.id == other)
                        .map(|a| a.group.clone())
                        .unwrap_or_default(),
                    None => String::new(),
                };
                m.selected = Some(id.clone());
                m.edit(|c| {
                    if let Some(a) = c.actions.actions.iter_mut().find(|a| a.id == id) {
                        a.group = group;
                    }
                });
            }
        },
        move || edit_action(&me2),
    );

    // Triggers.
    let mut tr = triggers.clone();
    tr.set_callback({
        let me = me.clone();
        move |b| {
            if app::event_clicks() && b.value() > 0 {
                edit_trigger(&me);
            }
        }
    });
    on(
        &tb[0],
        Box::new(|m| {
            if m.selected.is_none() {
                return;
            }
            let names: Vec<String> = TRIGGERS.iter().map(|(_, n)| n.to_string()).collect();
            let Some(i) = dialogs::pick("Add a trigger", &names) else {
                return;
            };
            let Some(t) = fill_new(m, edit::new_trigger(TRIGGERS[i].0), true) else {
                return;
            };
            m.edit_action(|a| a.triggers.push(t));
        }),
    );
    {
        let me2 = me.clone();
        let mut b = tb[1].clone();
        b.set_callback(move |_| edit_trigger(&me2));
    }
    on(
        &tb[2],
        Box::new(|m| {
            let i = m.triggers.value() - 1;
            m.edit_action(|a| {
                if let Some(t) = a.triggers.get_mut(i.max(0) as usize).filter(|_| i >= 0) {
                    let on = t.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    t.insert("enabled".into(), (!on).into());
                }
            });
        }),
    );
    on(
        &tb[3],
        Box::new(|m| {
            let (Some(id), i) = (m.selected.clone(), m.triggers.value() - 1) else {
                return;
            };
            let t = m
                .cfg
                .borrow()
                .actions
                .actions
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| a.triggers.get(i.max(0) as usize).filter(|_| i >= 0).cloned());
            let Some(t) = t else {
                return dialog::alert_default("Select a trigger to test.");
            };
            let hint = "Arguments to run the action with, as this trigger would pass them:";
            let Some(Value::Object(mut args)) = dialogs::json_editor("Test trigger", hint, &json!({}), |v| {
                if v.is_object() {
                    Ok(())
                } else {
                    Err("The arguments must be a JSON object.".into())
                }
            }) else {
                return;
            };
            let k = t.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
            args.insert("triggerId".into(), s(&t, "id").into());
            args.insert("triggerName".into(), name(TRIGGERS, k).unwrap_or("").into());
            args.insert("isTest".into(), true.into());
            super::tell(&m.cmds, move |e| drop(e.queue(&id, args)));
        }),
    );
    on(
        &tb[4],
        Box::new(|m| {
            let i = m.triggers.value() - 1;
            if i >= 0 {
                m.edit_action(|a| {
                    if (i as usize) < a.triggers.len() {
                        a.triggers.remove(i as usize);
                    }
                });
            }
        }),
    );

    // Sub-actions.
    on(
        &sb[0],
        Box::new(|m| {
            if m.selected.is_none() {
                return;
            }
            let names: Vec<String> = SUB_ACTION_INFO
                .iter()
                .filter(|x| !(99901..=99904).contains(&x.0) || x.0 == 99903)
                .map(|(_, n, c, _)| {
                    if c.is_empty() {
                        n.to_string()
                    } else {
                        format!("{c}: {n}")
                    }
                })
                .collect();
            let infos: Vec<u32> = SUB_ACTION_INFO
                .iter()
                .filter(|x| !(99901..=99904).contains(&x.0) || x.0 == 99903)
                .map(|x| x.0)
                .collect();
            let Some(i) = dialogs::pick("Add a sub-action", &names) else {
                return;
            };
            let kind = infos[i] as u64;
            let mut o = edit::new_sub(kind);
            if kind == 99903 {
                o.insert("values".into(), json!([]));
            }
            // The form's fields go before the children and the base fields.
            let kids = o.shift_remove("subActions");
            let Some(mut o) = fill_new(m, o, false) else {
                return;
            };
            if let Some(kids) = kids {
                let at = o.keys().position(|k| k == "id").unwrap_or(o.len());
                o.shift_insert(at, "subActions".into(), kids);
            }
            let at = m.selected_sub();
            m.edit_action(|a| {
                let list = &mut a.sub_actions;
                let (parent, pos) = match at
                    .as_deref()
                    .and_then(|p| edit::get(list, p).map(|o| (p, edit::kind(o))))
                {
                    // Switch cases go into the Switch.
                    Some((p, k)) if kind == 99903 && k == 127 => (p.to_vec(), usize::MAX),
                    Some((p, k)) if edit::holds_any(k) && kind != 99903 => (p.to_vec(), usize::MAX),
                    Some((p, _)) => {
                        let (last, parent) = p.split_last().expect("a path");
                        (parent.to_vec(), last + 1)
                    }
                    None => (Vec::new(), usize::MAX),
                };
                edit::insert(list, &parent, pos, o);
            });
        }),
    );
    {
        let me2 = me.clone();
        let mut b = sb[1].clone();
        b.set_callback(move |_| edit_sub(&me2));
    }
    on(
        &sb[2],
        Box::new(|m| {
            let Some(p) = m.selected_sub() else { return };
            m.edit_action(|a| {
                edit::update(&mut a.sub_actions, &p, |o| {
                    let on = o.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    o.insert("enabled".into(), (!on).into());
                });
            });
        }),
    );
    on(
        &sb[3],
        Box::new(|m| {
            let Some(p) = m.selected_sub() else { return };
            let fixed = m
                .selected
                .as_ref()
                .and_then(|id| {
                    m.cfg
                        .borrow()
                        .actions
                        .actions
                        .iter()
                        .find(|a| &a.id == id)
                        .and_then(|a| edit::get(&a.sub_actions, &p).map(|o| edit::fixed(edit::kind(o))))
                })
                .unwrap_or(true);
            if fixed {
                return dialog::alert_default("This branch is part of its If/Else or Switch; delete that instead.");
            }
            m.edit_action(|a| {
                edit::remove(&mut a.sub_actions, &p);
            });
        }),
    );
    drag(
        &subs,
        {
            let me = me.clone();
            move |from, to| {
                let mut m = me.borrow_mut();
                let Some(from) = m.sub_path(&from) else { return };
                let to = to.and_then(|t| m.sub_path(&t));
                m.edit_action(|a| {
                    let l = &mut a.sub_actions;
                    let target = match &to {
                        Some(t) => edit::drop_target(l, t),
                        None => Some((Vec::new(), usize::MAX)),
                    };
                    if let Some((parent, at)) = target {
                        edit::move_to(l, &from, &parent, at);
                    }
                });
            }
        },
        {
            let me = me.clone();
            move || edit_sub(&me)
        },
    );
    me
}

/// Dragging in a tree: `done(dragged, target)` when the mouse is let go over `target` (None:
/// below the last item). FLTK's own handling runs first (the selection follows the mouse, which
/// shows where it lands), so the dragged item is the one selected on the press.
fn drag(t: &Tree, done: impl Fn(TreeItem, Option<TreeItem>) + 'static, double: impl Fn() + 'static) {
    let mut t = t.clone();
    let mut start: Option<(TreeItem, i32)> = None;
    let mut dragging = false;
    t.handle(move |t, ev| match ev {
        Event::Push => {
            start = t.first_selected_item().map(|i| (i, app::event_y()));
            dragging = false;
            if app::event_clicks() && start.is_some() {
                start = None;
                double();
                return true;
            }
            false
        }
        Event::Drag => {
            if let Some((_, y)) = &start {
                if !dragging && (app::event_y() - y).abs() > 4 {
                    dragging = true;
                    draw::set_cursor(Cursor::Move);
                }
            }
            false
        }
        Event::Released => {
            let from = start.take();
            if !std::mem::take(&mut dragging) {
                return false;
            }
            draw::set_cursor(Cursor::Default);
            let Some((from, _)) = from else { return false };
            let to = t.find_clicked(true);
            if to.as_ref() != Some(&from) {
                done(from, to);
            }
            true
        }
        _ => false,
    });
}

fn edit_action(me: &Rc<RefCell<Actions>>) {
    let mut m = me.borrow_mut();
    let Some(id) = m.selected.clone() else { return };
    let (a, groups, queues) = {
        let c = m.cfg.borrow();
        let Some(a) = c.actions.actions.iter().find(|a| a.id == id).cloned() else {
            return;
        };
        let mut groups: Vec<String> = c
            .actions
            .actions
            .iter()
            .map(|a| a.group.clone())
            .filter(|g| !g.is_empty())
            .collect();
        groups.sort_by_key(|g| g.to_lowercase());
        groups.dedup();
        let mut queues = vec![(DEFAULT_QUEUE.to_string(), "Default".to_string())];
        queues.extend(
            c.actions
                .queues
                .iter()
                .map(|q| (s(q, "id").to_string(), s(q, "name").to_string())),
        );
        (a, groups, queues)
    };
    if let Some(new) = dialogs::action(&a, &groups, &queues) {
        m.edit(|c| {
            if let Some(x) = c.actions.actions.iter_mut().find(|x| x.id == id) {
                *x = new;
            }
        });
    }
}

/// Edit a sub-action or trigger: in its form, or as JSON (no form, or asked for). The type stays.
fn edit_obj(m: &Actions, title: &str, fields: Option<&[forms::F]>, o: &Obj, hint: &str) -> Option<Obj> {
    if let Some(fields) = fields {
        let lists = Lists::new(&m.cfg.borrow());
        match forms::form(title, fields, o, &lists) {
            Done::Ok(x) => return Some(x),
            Done::Cancel => return None,
            Done::Json => {}
        }
    }
    let k = edit::kind(o);
    let new = dialogs::json_editor(title, hint, &Value::Object(o.clone()), |v| {
        match v.get("type").and_then(Value::as_u64) {
            Some(x) if x == k => Ok(()),
            _ => Err(format!("Keep \"type\": {k}.")),
        }
    });
    match new {
        Some(Value::Object(x)) => Some(x),
        _ => None,
    }
}

fn trigger_title(k: u64) -> &'static str {
    name(TRIGGERS, k as u32).unwrap_or("Trigger")
}

fn sub_title(k: u64) -> &'static str {
    SUB_ACTION_INFO
        .binary_search_by_key(&(k as u32), |x| x.0)
        .map_or("Sub-action", |i| SUB_ACTION_INFO[i].1)
}

/// A new trigger or sub-action with fields opens its form first; cancelling it adds nothing.
fn fill_new(m: &Actions, o: Obj, trigger: bool) -> Option<Obj> {
    let k = edit::kind(&o);
    let (title, fields) = if trigger {
        (trigger_title(k), forms::trigger_fields(k))
    } else {
        (sub_title(k), forms::sub_fields(k))
    };
    match fields {
        Some(f) if !f.is_empty() => edit_obj(m, title, Some(f), &o, ""),
        _ => Some(o),
    }
}

fn edit_trigger(me: &Rc<RefCell<Actions>>) {
    let mut m = me.borrow_mut();
    let (Some(id), i) = (m.selected.clone(), m.triggers.value() - 1) else {
        return;
    };
    if i < 0 {
        return;
    }
    let t = m
        .cfg
        .borrow()
        .actions
        .actions
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.triggers.get(i as usize).cloned());
    let Some(t) = t else { return };
    let k = edit::kind(&t);
    let title = trigger_title(k);
    let hint = format!("{title} trigger. Its fields are in Streamer.bot's docs.");
    if let Some(mut new) = edit_obj(&m, title, forms::trigger_fields(k), &t, &hint) {
        if let Some(keep) = t.get("id") {
            new.insert("id".into(), keep.clone());
        }
        m.edit_action(|a| {
            if let Some(t) = a.triggers.get_mut(i as usize) {
                *t = new;
            }
        });
    }
}

fn edit_sub(me: &Rc<RefCell<Actions>>) {
    let mut m = me.borrow_mut();
    let (Some(id), Some(p)) = (m.selected.clone(), m.selected_sub()) else {
        return;
    };
    let o = m
        .cfg
        .borrow()
        .actions
        .actions
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| edit::get(&a.sub_actions, &p).cloned());
    let Some(mut o) = o else { return };
    // Children are edited in the list, not here.
    let kids = o.shift_remove("subActions");
    let k = edit::kind(&o);
    let title = sub_title(k);
    let hint = match SUB_ACTION_INFO.binary_search_by_key(&(k as u32), |x| x.0) {
        Ok(i) => format!("{title}. Its fields: {}", SUB_ACTION_INFO[i].3.join(", ")),
        Err(_) => title.into(),
    };
    if let Some(mut new) = edit_obj(&m, title, forms::sub_fields(k), &o, &hint) {
        for f in ["id", "parentId", "index"] {
            if let Some(v) = o.get(f) {
                new.insert(f.into(), v.clone());
            }
        }
        // Children go back where Streamer.bot keeps them: after its own fields, before the id.
        if let Some(kids) = kids {
            let at = new.keys().position(|k| k == "id").unwrap_or(new.len());
            new.shift_insert(at, "subActions".into(), kids);
        }
        m.edit_action(|a| {
            edit::update(&mut a.sub_actions, &p, |x| *x = new);
        });
    }
}
