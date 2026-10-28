//! The Actions tab, as in Streamer.bot: actions by group on the left; the selected action's
//! triggers and sub-actions on the right. Drag an action onto a group (or another action) to move it
//! there; drag a sub-action to reorder it or into a container (group, branch, loop).

use super::fit::Ellipsis;
use super::forms::{self, Done, Lists};
use super::rows::Rows;
use super::{button, dialogs, ROW};
use crate::edit::{self, Path};
use crate::engine::plugins::{SUB_PLUGIN, T_PLUGIN};
use crate::engine::{UiCmd, DEFAULT_QUEUE};
use crate::i18n::{tr, trf};
use crate::model::Obj;
use crate::types::{name, SUB_ACTION_INFO, TRIGGERS};
use crate::Config;
use fltk::button::Button;
use fltk::enums::{Align, Color, Cursor, Event, Font, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, Wizard};
use fltk::input::Input;
use fltk::prelude::*;
use fltk::tree::{Tree, TreeItem, TreeReason, TreeSelect};
use fltk::{app, draw};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, watch};

pub struct Actions {
    cfg: Arc<watch::Sender<Config>>,
    cmds: UnboundedSender<UiCmd>,
    tree: Tree,
    triggers: Rows,
    subs: Tree,
    title: Frame,
    search_actions: Input,
    search_subs: Input,
    /// The selected action's id.
    selected: Option<String>,
    /// Tree items and what they are: an action id, or a group name.
    items: Vec<(TreeItem, Node)>,
    sub_items: Vec<(TreeItem, Path)>,
    detail_title: Frame,
    detail_desc: Frame,
    action_btns: Vec<Button>,
    trigger_btns: Vec<Button>,
    sub_btns: Vec<Button>,
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
    if k == SUB_PLUGIN {
        return format!("{}: {}", s(o, "plugin"), s(o, "name"));
    }
    let what = SUB_ACTION_INFO.binary_search_by_key(&(k as u32), |x| x.0).map_or_else(
        |_| {
            name(crate::types::SUB_ACTIONS, k as u32)
                .unwrap_or(tr("actions.unknown"))
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
        out += tr("actions.not_supported");
    }
    out
}

pub fn trigger_label(t: &Obj, names: &Names) -> String {
    let k = t.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
    if k == T_PLUGIN {
        return format!("{}: {}", s(t, "plugin"), s(t, "event"));
    }
    let what = if let Some(raw) = name(TRIGGERS, k) {
        let cat = super::picker::trigger_categorize(raw);
        let platform = cat.split("/").next().unwrap_or(&cat);
        let clean = super::picker::trigger_clean_title(raw);
        format!("{platform}: {clean}")
    } else {
        tr("actions.unknown").to_string()
    };
    let n = |f: &str| t.get(f).and_then(Value::as_i64).unwrap_or(0);
    let detail = if k == crate::hotkeys::T_KEY_PRESS {
        crate::hotkeys::label(n("keyCode"), n("modifierMask"))
    } else {
        t.iter()
            .filter(|(k, _)| !["id", "type", "enabled", "exclusions"].contains(&k.as_str()))
            .filter_map(|(k, x)| {
                let x = crate::vars::text(x);
                let x = names.get(&x).cloned().unwrap_or(x);
                (!x.is_empty()).then(|| format!("{}: {x}", k.trim_end_matches("Id")))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let on = t.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let mut out = if detail.is_empty() {
        what.to_string()
    } else {
        format!("{what} ({})", short(&detail))
    };
    if !on {
        out += tr("actions.disabled_tag");
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

    fn update_details_trigger(&mut self, trigger_idx: usize) {
        let Some(ref id) = self.selected else { return };
        let cfg = self.cfg.borrow();
        let Some(a) = cfg.actions.actions.iter().find(|a| &a.id == id) else {
            return;
        };
        let Some(t) = a.triggers.get(trigger_idx) else { return };
        let k = t.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
        let raw = name(TRIGGERS, k).unwrap_or(tr("actions.unknown"));
        let cat = super::picker::trigger_categorize(raw);
        let title = super::picker::trigger_clean_title(raw);
        let desc = super::picker::trigger_description(k, raw, &title, &cat);
        self.detail_title.set_label(&trf(
            "actions.trigger_details",
            &[("title", &title), ("category", &cat.replace('/', " > "))],
        ));
        self.detail_desc.set_label(&desc);
        self.detail_title.redraw();
        self.detail_desc.redraw();
    }

    fn update_details_sub(&mut self, path: &[usize]) {
        let Some(ref id) = self.selected else { return };
        let cfg = self.cfg.borrow();
        let Some(a) = cfg.actions.actions.iter().find(|a| &a.id == id) else {
            return;
        };
        let Some(o) = edit::get(&a.sub_actions, path) else {
            return;
        };
        let k = edit::kind(o) as u32;
        let (name, cat) = SUB_ACTION_INFO
            .iter()
            .find(|x| x.0 == k)
            .map(|x| (x.1, x.2))
            .unwrap_or((tr("actions.subaction_word"), "Core"));
        let cat_str = if cat.is_empty() { "Core" } else { cat };
        let desc = super::picker::sub_action_description(k, name, cat_str);
        self.detail_title.set_label(&trf(
            "actions.subaction_details",
            &[("name", name), ("category", &cat_str.replace('/', " > "))],
        ));
        self.detail_desc.set_label(&desc);
        self.detail_title.redraw();
        self.detail_desc.redraw();
    }

    fn update_details_action(&mut self) {
        if let Some(ref id) = self.selected {
            let cfg = self.cfg.borrow();
            if let Some(a) = cfg.actions.actions.iter().find(|a| &a.id == id) {
                let grp = if a.group.is_empty() {
                    tr("actions.none_group")
                } else {
                    &a.group
                };
                self.detail_title.set_label(&trf(
                    "actions.action_details_title",
                    &[("name", &a.name), ("group", grp)],
                ));
                self.detail_desc.set_label(&trf(
                    "actions.action_details_desc",
                    &[
                        ("triggers", &a.triggers.len().to_string()),
                        ("subactions", &a.sub_actions.len().to_string()),
                    ],
                ));
                self.detail_title.redraw();
                self.detail_desc.redraw();
                return;
            }
        }
        self.detail_title.set_label(tr("actions.details"));
        self.detail_desc.set_label(tr("actions.details_empty"));
        self.detail_title.redraw();
        self.detail_desc.redraw();
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
        let filter = self.search_actions.value().trim().to_lowercase();
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
        if !filter.is_empty() {
            sorted.retain(|a| a.name.to_lowercase().contains(&filter) || a.group.to_lowercase().contains(&filter));
        }
        sorted.sort_by_key(|a| a.name.to_lowercase());
        for g in &groups {
            let in_group: Vec<_> = sorted.iter().filter(|a| &a.group == g).collect();
            if !filter.is_empty() && in_group.is_empty() && !g.to_lowercase().contains(&filter) {
                continue;
            }
            let Some(mut gi) = self.tree.insert(&root, &plain(g), root.children()) else {
                continue;
            };
            gi.set_label_font(Font::HelveticaBold);
            self.items.push((gi.clone(), Node::Group(g.clone())));
            for a in in_group {
                add(&mut self.tree, &gi, a, &mut self.items);
            }
            if !filter.is_empty() {
                gi.open();
            } else if closed.contains(g) {
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
            self.title.set_label(tr("actions.pick_to_edit"));
            self.detail_title.set_label("");
            self.detail_desc.set_label("");
            for b in &mut self.action_btns {
                b.deactivate();
            }
            for b in &mut self.trigger_btns {
                b.deactivate();
            }
            for b in &mut self.sub_btns {
                b.deactivate();
            }
            self.subs.redraw();
            return;
        };
        for b in &mut self.action_btns {
            b.activate();
        }
        for b in &mut self.trigger_btns {
            b.activate();
        }
        for b in &mut self.sub_btns {
            b.activate();
        }
        let mut flags = Vec::new();
        for (on, what) in [
            (!a.enabled, tr("actions.flag_disabled")),
            (a.random_action, tr("actions.flag_random")),
            (a.concurrent, tr("actions.flag_concurrent")),
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
        let sub_filter = self.search_subs.value().trim().to_lowercase();
        for t in &a.triggers {
            let lbl = trigger_label(t, &names);
            if sub_filter.is_empty() || lbl.to_lowercase().contains(&sub_filter) {
                self.triggers.add(&lbl);
            }
        }
        if keep_trigger > 0 && keep_trigger <= self.triggers.size() {
            self.triggers.select(keep_trigger);
        }
        let Some(root) = self.subs.root() else { return };
        fn matches_filter(o: &Obj, names: &Names, q: &str) -> bool {
            if q.is_empty() || sub_label(o, names).to_lowercase().contains(q) {
                return true;
            }
            if let Some(kids) = o.get("subActions").and_then(Value::as_array) {
                kids.iter()
                    .filter_map(|x| x.as_object())
                    .any(|k| matches_filter(k, names, q))
            } else {
                false
            }
        }
        fn add(
            t: &mut Tree,
            parent: &TreeItem,
            list: &[Obj],
            path: &mut Path,
            items: &mut Vec<(TreeItem, Path)>,
            names: &Names,
            sub_filter: &str,
        ) {
            for (i, o) in list.iter().enumerate() {
                path.push(i);
                if !sub_filter.is_empty() && !matches_filter(o, names, sub_filter) {
                    path.pop();
                    continue;
                }
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
                    add(t, &it, &kids, path, items, names, sub_filter);
                    if !sub_filter.is_empty() {
                        it.open();
                    }
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
            &sub_filter,
        );
        if let Some(p) = keep {
            if let Some((i, _)) = self.sub_items.iter().find(|(_, x)| *x == p) {
                let _ = self.subs.select_only(i, false);
            }
        }
        self.subs.redraw();
        self.update_details_action();
    }
}

impl Config {
    fn clone_actions(&self) -> Vec<crate::model::Action> {
        self.actions.actions.clone()
    }
}

fn bar(col: &mut Flex, labels: &[&str]) -> Vec<Button> {
    super::grid(col, labels, 1)
}

/// A tree saying `empty` while it has nothing in it.
fn tree(empty: &'static str) -> Tree {
    let mut t = Tree::default();
    t.set_show_root(false);
    t.set_select_mode(TreeSelect::Single);
    t.draw(move |t| {
        if t.root().is_none_or(|r| r.children() == 0) {
            fltk::draw::set_font(Font::Helvetica, 13);
            fltk::draw::set_draw_color(super::muted());
            fltk::draw::draw_text2(empty, t.x() + 10, t.y(), t.w() - 20, t.h(), Align::Center | Align::Wrap);
        }
    });
    t
}

/// Ask for an action name.
fn ask_name(title: &str, current: &str) -> Option<String> {
    super::ask(title, tr("actions.ask_name"), current)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn tab(tabs: &Wizard, cfg: Arc<watch::Sender<Config>>, cmds: UnboundedSender<UiCmd>) -> Rc<RefCell<Actions>> {
    let mut row = Flex::new(tabs.x(), tabs.y(), tabs.w(), tabs.h(), None)
        .row()
        .with_label("Actions");
    row.set_margin(8);
    row.set_pad(8);
    let mut left = Flex::default().column();
    left.set_pad(4);
    let mut act_head = Flex::default().row();
    act_head.set_pad(4);
    let t = Frame::default()
        .with_label("Actions")
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    act_head.fixed(&t, 60);
    let mut search_actions = Input::default();
    search_actions.set_tooltip(tr("actions.filter_actions"));
    super::placeholder(&mut search_actions, tr("actions.search_actions"));
    act_head.end();
    left.fixed(&act_head, ROW);
    let actions_tree = tree(tr("actions.empty_actions"));
    let all = super::grid(
        &mut left,
        &[
            tr("btn.add"),
            tr("btn.edit"),
            tr("btn.duplicate"),
            tr("btn.delete"),
            tr("btn.test"),
            tr("btn.import"),
            tr("btn.export"),
            tr("btn.templates"),
        ],
        3,
    );
    let (lb, lb2, lb3) = (all[0..3].to_vec(), all[3..5].to_vec(), all[5..].to_vec());
    left.end();
    row.fixed(&left, 340);
    let mut right = Flex::default().column();
    right.set_pad(4);
    let mut title = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    title.set_label_font(Font::HelveticaBold);
    right.fixed(&title, ROW - 2);
    let t = Frame::default()
        .with_label("Triggers")
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    right.fixed(&t, ROW - 4);
    let triggers = Rows::new(tr("actions.empty_triggers"));
    right.fixed(&*triggers, 120);
    let tb = bar(
        &mut right,
        &[
            tr("btn.add"),
            tr("btn.edit"),
            tr("btn.on_off"),
            tr("btn.test"),
            tr("btn.delete"),
        ],
    );
    let mut sub_head = Flex::default().row();
    sub_head.set_pad(4);
    let t = Frame::default()
        .with_label("Sub-actions")
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    sub_head.fixed(&t, 90);
    let mut search_subs = Input::default();
    search_subs.set_tooltip(tr("actions.filter_subactions"));
    super::placeholder(&mut search_subs, tr("actions.search_subactions"));
    sub_head.end();
    right.fixed(&sub_head, ROW);
    let subs = tree(tr("actions.empty_subactions"));
    let sb = bar(
        &mut right,
        &[tr("btn.add"), tr("btn.edit"), tr("btn.on_off"), tr("btn.delete")],
    );

    let mut detail_title = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    detail_title.set_label_font(Font::HelveticaBold);
    detail_title.set_label_color(Color::from_u32(0x5599ff));
    right.fixed(&detail_title, 18);
    let mut detail_desc = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap);
    detail_desc.set_label_color(Color::from_u32(0x888888));
    right.fixed(&detail_desc, 38);

    right.end();
    row.end();

    let me = Rc::new(RefCell::new(Actions {
        cfg,
        cmds,
        tree: actions_tree.clone(),
        triggers: triggers.clone(),
        subs: subs.clone(),
        title,
        search_actions: search_actions.clone(),
        search_subs: search_subs.clone(),
        selected: None,
        items: Vec::new(),
        sub_items: Vec::new(),
        detail_title: detail_title.clone(),
        detail_desc: detail_desc.clone(),
        action_btns: vec![lb[1].clone(), lb[2].clone(), lb2[0].clone(), lb2[1].clone()],
        trigger_btns: tb.clone(),
        sub_btns: sb.clone(),
    }));

    let mut sa = search_actions.clone();
    sa.set_trigger(fltk::enums::CallbackTrigger::Changed);
    sa.set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh();
            }
        }
    });

    let mut ss = search_subs.clone();
    ss.set_trigger(fltk::enums::CallbackTrigger::Changed);
    ss.set_callback({
        let me = me.clone();
        move |_| {
            if let Ok(mut m) = me.try_borrow_mut() {
                m.refresh_action();
            }
        }
    });
    let on = |b: &Button, f: Box<dyn Fn(&mut Actions)>| {
        let me = me.clone();
        let mut b = b.clone();
        b.set_callback(move |_| f(&mut me.borrow_mut()));
    };

    // Right clicks: what's under the mouse, then its buttons.
    triggers.clone().set_menu(&tb);

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
            match m.node(&item) {
                Some(Node::Action(id)) => {
                    m.selected = Some(id);
                    m.refresh_action();
                }
                _ => {
                    m.selected = None;
                    m.refresh_action();
                }
            }
        }
    });
    on(
        &lb[0],
        Box::new(|m| {
            let Some(name) = ask_name(tr("actions.new_action"), "") else {
                return;
            };
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
            let src = m.cfg.borrow().actions.actions.iter().find(|a| a.id == id).cloned();
            let Some(src) = src else { return };
            let default_name = trf("actions.copy_suffix", &[("name", &src.name)]);
            let Some(name) = ask_name(tr("actions.duplicate_action"), &default_name) else {
                return;
            };
            let dup = edit::duplicate_action(&src, &name);
            let new_id = dup.id.clone();
            m.selected = Some(new_id);
            m.edit(|c| c.actions.actions.push(dup));
        }),
    );
    on(
        &lb2[0],
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
            if super::dialogs::choice(
                &trf("actions.delete_confirm", &[("name", &plain(&name))]),
                tr("dialogs.cancel"),
                tr("btn.delete"),
            ) {
                m.selected = None;
                m.edit(|c| c.actions.actions.retain(|a| a.id != id));
            }
        }),
    );
    on(
        &lb2[1],
        Box::new(|m| {
            if let Some(id) = m.selected.clone() {
                super::tell(&m.cmds, move |e| drop(e.test(&id)));
            }
        }),
    );
    on(
        &lb3[0],
        Box::new(|m| {
            let Some(x) = dialogs::import() else { return };
            let progs = edit::programs(&x);
            let warn = if progs.is_empty() {
                String::new()
            } else {
                let mut list: Vec<String> = progs.iter().take(8).cloned().collect();
                if progs.len() > 8 {
                    list.push(trf("actions.import_more", &[("count", &(progs.len() - 8).to_string())]));
                }
                trf("actions.import_warn", &[("programs", &list.join("\n"))])
            };
            let q = trf(
                "actions.import_confirm",
                &[("summary", &edit::summary(&x)), ("warn", &warn)],
            );
            if !super::dialogs::choice(&plain(&q), tr("dialogs.cancel"), tr("btn.import")) {
                return;
            }
            let mut result = Ok(Vec::new());
            m.edit(|c| result = edit::import(c, &x));
            match result {
                Ok(u) if u.is_empty() => crate::say(tr("actions.imported_toast").into()),
                Ok(u) => {
                    let msg = trf("actions.imported_unsupported", &[("items", &u.join(", "))]);
                    crate::warn(msg.clone());
                    super::dialogs::message(&plain(&msg));
                }
                Err(e) => super::dialogs::alert(&plain(&e)),
            }
        }),
    );
    {
        let me2 = me.clone();
        let mut b = lb3[2].clone();
        b.set_tooltip(tr("actions.templates_tooltip"));
        b.set_callback(move |_| {
            let Some(t) = pick_template() else { return };
            let made = (t.make)();
            let names: Vec<String> = made
                .actions
                .iter()
                .map(|a| a.name.clone())
                .chain(
                    made.commands
                        .iter()
                        .map(|c| format!("command {}", c["command"].as_str().unwrap_or(""))),
                )
                .chain(
                    made.timers
                        .iter()
                        .map(|x| format!("timer {}", x["name"].as_str().unwrap_or(""))),
                )
                .collect();
            let timers = !made.timers.is_empty();
            let mut m = me2.borrow_mut();
            m.selected = made.actions.first().map(|a| a.id.clone());
            m.edit(|c| {
                c.actions.actions.extend(made.actions);
                c.commands.extend(made.commands);
                c.timers.extend(made.timers);
            });
            if timers {
                super::tell(&m.cmds, |e| e.timers());
            }
            super::changed();
            crate::say(trf(
                "actions.added_from_template",
                &[("name", t.name), ("items", &names.join(", "))],
            ));
        });
    }
    on(
        &lb3[1],
        Box::new(|m| {
            let list: Vec<(String, String, String)> = {
                let c = m.cfg.borrow();
                let mut l: Vec<(String, String, String)> = c
                    .actions
                    .actions
                    .iter()
                    .map(|a| (a.id.clone(), a.name.clone(), a.group.clone()))
                    .collect();
                l.sort_by(|x, y| {
                    x.2.to_lowercase()
                        .cmp(&y.2.to_lowercase())
                        .then_with(|| x.1.to_lowercase().cmp(&y.1.to_lowercase()))
                });
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
    let me_del_act = me.clone();
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
        Some(Box::new(move |k| {
            if k == Key::Delete || k == Key::BackSpace {
                let mut m = me_del_act.borrow_mut();
                let Some(id) = m.selected.clone() else { return false };
                let name = m
                    .cfg
                    .borrow()
                    .actions
                    .actions
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                if super::dialogs::choice(
                    &trf("actions.delete_confirm", &[("name", &plain(&name))]),
                    tr("dialogs.cancel"),
                    tr("btn.delete"),
                ) {
                    m.selected = None;
                    m.edit(|c| c.actions.actions.retain(|a| a.id != id));
                }
                true
            } else {
                false
            }
        })),
        all.clone(),
    );

    // Triggers.
    let mut tr_rows = triggers.clone();
    let me_tr = me.clone();
    let me_key_tr = me.clone();
    tr_rows.set_callback(move |b| {
        if b.value() > 0 {
            if let Ok(mut m) = me_tr.try_borrow_mut() {
                m.update_details_trigger((b.value() - 1) as usize);
            }
            if app::event_clicks() {
                edit_trigger(&me_tr);
            }
        }
    });
    tr_rows.set_keys(move |b, key| {
        let i = b.value() - 1;
        if i < 0 {
            return false;
        }
        if key == Key::Enter {
            edit_trigger(&me_key_tr);
        } else if key == Key::Delete || key == Key::BackSpace {
            me_key_tr.borrow_mut().edit_action(|a| {
                if (i as usize) < a.triggers.len() {
                    a.triggers.remove(i as usize);
                }
            });
        } else if key == Key::from_char(' ') {
            me_key_tr.borrow_mut().edit_action(|a| {
                if let Some(t) = a.triggers.get_mut(i as usize) {
                    let on = t.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    t.insert("enabled".into(), (!on).into());
                }
            });
        } else {
            return false;
        }
        true
    });
    on(
        &tb[0],
        Box::new(|m| {
            if m.selected.is_none() {
                super::dialogs::alert(tr("actions.select_action_first"));
                return;
            }
            let mut items = super::picker::trigger_pick_items();
            let (more, what) = plugin_items(m, true);
            items.extend(more);
            let Some(code) = super::picker::pick_tree(tr("actions.add_trigger"), &items) else {
                return;
            };
            let new = match what.get(code.wrapping_sub(PLUGIN_PICK)) {
                Some((plugin, event)) => {
                    let mut t = edit::new_trigger(T_PLUGIN);
                    t.insert("plugin".into(), plugin.as_str().into());
                    t.insert("event".into(), event.as_str().into());
                    t
                }
                None => edit::new_trigger(code as u32),
            };
            let Some(t) = fill_new(m, new, true) else {
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
                return super::dialogs::alert(tr("actions.select_trigger_first"));
            };
            let hint = tr("actions.test_trigger_hint");
            let Some(mut args) = dialogs::values(tr("actions.test_trigger_title"), hint, &Obj::new(), true) else {
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
                super::dialogs::alert(tr("actions.select_action_first"));
                return;
            }
            let mut items = super::picker::sub_action_pick_items();
            let (more, what) = plugin_items(m, false);
            items.extend(more);
            let Some(kind) = super::picker::pick_tree(tr("actions.add_subaction"), &items) else {
                return;
            };
            let plugin = what.get(kind.wrapping_sub(PLUGIN_PICK)).cloned();
            let kind = if plugin.is_some() { SUB_PLUGIN } else { kind as u64 };
            let mut o = edit::new_sub(kind);
            if let Some((p, n)) = plugin {
                o.insert("plugin".into(), p.into());
                o.insert("name".into(), n.into());
            }
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
                return super::dialogs::alert(tr("actions.branch_delete_warning"));
            }
            m.edit_action(|a| {
                edit::remove(&mut a.sub_actions, &p);
            });
        }),
    );
    let mut s_tree = subs.clone();
    s_tree.set_callback({
        let me = me.clone();
        move |t| {
            let Some(item) = t.callback_item() else { return };
            if t.callback_reason() != TreeReason::Selected {
                return;
            }
            let Ok(mut m) = me.try_borrow_mut() else { return };
            if let Some(path) = m.sub_path(&item) {
                m.update_details_sub(&path);
            }
        }
    });

    let me_key_sub = me.clone();
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
        Some(Box::new(move |k| {
            if k == Key::Delete || k == Key::BackSpace {
                let mut m = me_key_sub.borrow_mut();
                let Some(p) = m.selected_sub() else { return false };
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
                    super::dialogs::alert(tr("actions.branch_delete_warning"));
                    return true;
                }
                m.edit_action(|a| {
                    edit::remove(&mut a.sub_actions, &p);
                });
                true
            } else if k == Key::from_char(' ') {
                let mut m = me_key_sub.borrow_mut();
                let Some(p) = m.selected_sub() else { return false };
                m.edit_action(|a| {
                    edit::update(&mut a.sub_actions, &p, |o| {
                        let on = o.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                        o.insert("enabled".into(), (!on).into());
                    });
                });
                true
            } else {
                false
            }
        })),
        sb.clone(),
    );
    me
}

/// Dragging in a tree: `done(dragged, target)` when the mouse is let go over `target` (None:
/// below the last item). FLTK's own handling runs first (the selection follows the mouse, which
/// shows where it lands), so the dragged item is the one selected on the press.
fn drag(
    t: &Tree,
    done: impl Fn(TreeItem, Option<TreeItem>) + 'static,
    double: impl Fn() + 'static,
    key_action: Option<Box<dyn Fn(Key) -> bool + 'static>>,
    menu: Vec<Button>,
) {
    let mut t = t.clone();
    let mut start: Option<(TreeItem, i32)> = None;
    let mut dragging = false;
    t.handle(move |t, ev| match ev {
        Event::Push if app::event_mouse_button() == app::MouseButton::Right => {
            if let Some(item) = t.find_clicked(true) {
                let _ = t.select_only(&item, true);
            }
            super::context_menu(&menu);
            true
        }
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
        Event::KeyDown => {
            let key = app::event_key();
            if key == Key::Enter {
                double();
                return true;
            }
            if let Some(ref kf) = key_action {
                if kf(key) {
                    return true;
                }
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
            _ => Err(trf("actions.json_editor_hint", &[("type", &k.to_string())])),
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
    let title = if trigger { trigger_title(k) } else { sub_title(k) };
    match fields_of(m, &o, trigger) {
        Some(f) if !f.is_empty() => edit_obj(m, title, Some(f), &o, ""),
        _ => Some(o),
    }
}

/// The form fields for `o`: its type's, or its plugin's.
fn fields_of(m: &Actions, o: &Obj, trigger: bool) -> Option<&'static [forms::F]> {
    let k = edit::kind(o);
    if !trigger && k == SUB_PLUGIN {
        let (id, n) = (s(o, "plugin").to_string(), s(o, "name").to_string());
        let on = super::query(&m.cmds, |e| e.plugins_on()).unwrap_or_default();
        let sub = on
            .iter()
            .find(|p| p.0 == id)?
            .1
            .subactions
            .iter()
            .find(|x| x.name == n)?
            .clone();
        return Some(forms::plugin_fields(&format!("{id}/{n}"), &sub.fields));
    }
    if trigger {
        forms::trigger_fields(k)
    } else {
        forms::sub_fields(k)
    }
}

/// The plugins' sub-actions or triggers as picker items (ids from `PLUGIN_PICK`), and what each is.
fn plugin_items(m: &Actions, trigger: bool) -> (Vec<super::picker::PickItem>, Vec<(String, String)>) {
    let on = super::query(&m.cmds, |e| e.plugins_on()).unwrap_or_default();
    let mut items = Vec::new();
    let mut what = Vec::new();
    for (id, man) in &on {
        let list: Vec<(String, String)> = if trigger {
            man.triggers.iter().map(|t| (t.name.clone(), t.about.clone())).collect()
        } else {
            man.subactions
                .iter()
                .map(|t| (t.name.clone(), t.about.clone()))
                .collect()
        };
        for (n, about) in list {
            items.push(super::picker::PickItem {
                id: PLUGIN_PICK + what.len(),
                title: n.clone(),
                category: format!("Plugins/{}", man.name),
                description: about,
            });
            what.push((id.clone(), n));
        }
    }
    (items, what)
}

/// Picker ids from here on are the plugins'.
const PLUGIN_PICK: usize = 1_000_000;

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
    let hint = trf("actions.trigger_hint", &[("title", title)]);
    if let Some(mut new) = edit_obj(&m, title, fields_of(&m, &t, true), &t, &hint) {
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
        Ok(i) => trf(
            "actions.subaction_fields_hint",
            &[("title", title), ("fields", &SUB_ACTION_INFO[i].3.join(", "))],
        ),
        Err(_) => title.into(),
    };
    if let Some(mut new) = edit_obj(&m, title, fields_of(&m, &o, false), &o, &hint) {
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

/// The template picker: each one's name and what it does. The one picked.
fn pick_template() -> Option<&'static crate::templates::Template> {
    use crate::templates::TEMPLATES;
    let mut win = fltk::window::Window::default()
        .with_size(560, 380)
        .with_label(tr("actions.template_title"));
    win.set_xclass("streamerz");
    win.make_modal(true);
    let mut col = Flex::new(10, 10, 540, 360, None).column();
    col.set_pad(6);
    let mut list = Rows::new("");
    for t in TEMPLATES {
        list.add(t.name);
    }
    list.select(1);
    let mut about = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap | Align::Top);
    about.set_label(TEMPLATES[0].about);
    col.fixed(&about, 60);
    let mut row = Flex::default().row();
    row.set_pad(8);
    Frame::default();
    let mut add = button(&mut row, tr("btn.add"), 90);
    let mut cancel = button(&mut row, tr("dialogs.cancel"), 90);
    row.end();
    col.fixed(&row, ROW + 4);
    col.end();
    win.end();
    let answer = Rc::new(std::cell::Cell::new(None::<bool>));
    list.set_callback({
        let mut about = about.clone();
        let a = answer.clone();
        move |l| {
            if let Some(t) = TEMPLATES.get((l.value() - 1).max(0) as usize) {
                about.set_label(&t.about.replace('@', "@@"));
            }
            // Double click adds.
            if fltk::app::event_clicks() {
                a.set(Some(true));
            }
        }
    });
    add.set_callback({
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
        if !super::nested_wait(None) {
            break;
        }
    }
    win.hide();
    let picked = answer.get() == Some(true) && list.value() > 0;
    picked.then(|| &TEMPLATES[(list.value() - 1) as usize])
}
