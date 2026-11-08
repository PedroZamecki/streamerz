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
use crate::groups;
use crate::i18n::{tr, trf};
use crate::model::Obj;
use crate::types::{name, SUB_ACTION_INFO, TRIGGERS};
use crate::Config;
use fltk::button::Button;
use fltk::enums::{Align, Color, Cursor, Event, Font, Key};
use fltk::frame::Frame;
use fltk::group::{Flex, FlexType, Wizard};
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
    group_pics: Vec<fltk::image::RgbImage>,
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
    let what = if let Some(title) = super::picker::spotify_player_title(k as u32) {
        title.to_string()
    } else if k == 99802 {
        tr("spotify.action_name").to_string()
    } else if k == 99803 {
        tr("countdown.action_name").to_string()
    } else if k == 99880 {
        tr("overlay.action_name").to_string()
    } else {
        SUB_ACTION_INFO.binary_search_by_key(&(k as u32), |x| x.0).map_or_else(
            |_| {
                name(crate::types::SUB_ACTIONS, k as u32)
                    .unwrap_or(tr("actions.unknown"))
                    .to_string()
            },
            |i| SUB_ACTION_INFO[i].1.to_string(),
        )
    };
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
        1002 => match o.get("value").and_then(Value::as_u64) {
            Some(ms) => crate::groups::format_duration(ms),
            None => v("value"),
        },
        200..=202 | 204..=210 | 213..=215 => {
            let id = v("rewardId");
            crate::catalog::name_of("twitch.rewards", &id).unwrap_or(id)
        }
        35101..=35103 | 35105..=35110 | 35113 => {
            let id = v("rewardId");
            crate::catalog::name_of("kick.rewards", &id).unwrap_or(id)
        }
        // Its clock is an ID nobody reads: say what it does to the clock instead.
        99803 => match v("operation").as_str() {
            "add" => tr("countdown.op_add").to_string(),
            "reset" => tr("countdown.op_reset").to_string(),
            other => other.to_string(),
        },
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
    let what = match k {
        706 => tr("automation.started").to_string(),
        707 => tr("automation.exiting").to_string(),
        _ if name(TRIGGERS, k).is_some() => {
            let raw = name(TRIGGERS, k).unwrap();
            let cat = super::picker::trigger_categorize(raw);
            let platform = cat.split('/').next().unwrap_or(&cat);
            let clean = super::picker::trigger_item_title(k, raw);
            format!("{platform}: {clean}")
        }
        _ => tr("actions.unknown").to_string(),
    };
    let n = |f: &str| t.get(f).and_then(Value::as_i64).unwrap_or(0);
    let detail = if k == crate::hotkeys::T_KEY_PRESS {
        crate::hotkeys::label(n("keyCode"), n("modifierMask"))
    } else if k == crate::tiktok::T_TIKTOK_GIFT {
        let mut parts = Vec::new();
        let id = t.get("giftId").map(crate::vars::text).unwrap_or_default();
        if !id.trim().is_empty() {
            parts.push(crate::catalog::name_of("tiktok.gifts", &id).unwrap_or_else(|| format!("#{id}")));
        }
        if t.get("giftDelivery").and_then(Value::as_i64) == Some(1) {
            parts.push(tr("automation.completed_combo").into());
        } else if !parts.is_empty() {
            parts.push(tr("automation.every_update").into());
        }
        let min = t.get("minCoins").and_then(Value::as_u64).unwrap_or(0);
        let max = t.get("maxCoins").and_then(Value::as_u64).unwrap_or(0);
        match (min, max) {
            (0, 0) => {}
            (lo, 0) => parts.push(format!("{lo}+ coins")),
            (0, hi) => parts.push(format!("<= {hi} coins")),
            (lo, hi) if lo == hi => parts.push(format!("{lo} coins")),
            (lo, hi) => parts.push(format!("{lo}-{hi} coins")),
        }
        parts.join(", ")
    } else if matches!(k, 112..=115 | 171 | 35024) {
        let id = t.get("rewardId").map(crate::vars::text).unwrap_or_default();
        let cat = if k == 35024 { "kick.rewards" } else { "twitch.rewards" };
        if id.trim().is_empty() {
            String::new()
        } else {
            crate::catalog::name_of(cat, &id).unwrap_or(id)
        }
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
    if detail.is_empty() {
        what.to_string()
    } else {
        format!("{what} ({})", short(&detail))
    }
}

/// Whether a trigger is switched on.
fn trigger_on(t: &Obj) -> bool {
    t.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

impl Actions {
    /// Change the config, save it, and redraw.
    fn edit(&mut self, f: impl FnOnce(&mut Config)) {
        self.cfg.send_modify(f);
        crate::save_later(&self.cfg.borrow());
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
        let title = super::picker::trigger_item_title(k, raw);
        let desc = super::picker::trigger_description(k, raw, &title, &cat);
        self.detail_title.set_label(&trf(
            "actions.trigger_details",
            &[("title", &title), ("category", &cat.replace('/', " > "))],
        ));
        self.detail_desc.set_label(&desc);
        self.show_details();
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
        self.show_details();
    }

    /// The details under the sub-actions show only when there is something to say: no blank room.
    fn show_details(&self) {
        let on = !self.detail_title.label().is_empty();
        for mut f in [self.detail_title.clone(), self.detail_desc.clone()] {
            if on {
                f.show();
            } else {
                f.hide();
            }
        }
        if let Some(mut right) = self.detail_title.parent().and_then(|p| Flex::from_dyn_widget(&p)) {
            right.layout();
            right.redraw();
        }
    }

    fn update_details_action(&mut self) {
        if let Some(ref id) = self.selected {
            let cfg = self.cfg.borrow();
            if let Some(a) = cfg.actions.actions.iter().find(|a| &a.id == id) {
                let title = if a.group.is_empty() {
                    a.name.clone()
                } else {
                    trf(
                        "actions.action_details_title",
                        &[("name", &a.name), ("group", &a.group)],
                    )
                };
                self.detail_title.set_label(&title.replace('@', "@@"));
                self.detail_desc.set_label(&trf(
                    "actions.action_details_desc",
                    &[
                        ("triggers", &a.triggers.len().to_string()),
                        ("subactions", &a.sub_actions.len().to_string()),
                    ],
                ));
                self.show_details();
                return;
            }
        }
        self.detail_title.set_label(tr("actions.details"));
        self.detail_desc.set_label(tr("actions.details_empty"));
        self.show_details();
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
        self.group_pics.clear();
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
                        i.set_label_fgcolor(super::muted());
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
            if let Some(bytes) = self
                .cfg
                .borrow()
                .action_group_icons
                .get(g)
                .and_then(crate::group_icons::decode)
            {
                let image = if bytes.starts_with(b"\x89PNG") {
                    fltk::image::PngImage::from_data(&bytes)
                        .ok()
                        .and_then(|i| i.to_rgb().ok())
                } else {
                    fltk::image::JpegImage::from_data(&bytes)
                        .ok()
                        .and_then(|i| i.to_rgb().ok())
                };
                if let Some(mut image) = image {
                    image.scale(18, 18, true, true);
                    gi.set_user_icon(Some(image.clone()));
                    self.group_pics.push(image);
                }
            }
            if gi.user_icon().is_none() {
                gi.set_user_icon(super::icons::icon("folder", super::muted(), 16));
            }
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
            self.show_details();
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
                let code = t.get("type").and_then(Value::as_u64).unwrap_or(0) as u32;
                self.triggers
                    .add_icon(&lbl, !trigger_on(t), super::picker::trigger_icon(code));
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
                    let off = o.get("enabled").and_then(Value::as_bool) == Some(false);
                    if off || !crate::engine::supported(k as u32) {
                        it.set_label_fgcolor(super::muted());
                    }
                    if off {
                        it.set_user_icon(super::icons::icon("power-off", super::muted(), 16));
                    } else {
                        it.set_user_icon(super::picker::sub_icon(k as u32));
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
    super::picker::style_tree(&mut t);
    t.draw(move |t| {
        if t.root().is_none_or(|r| r.children() == 0) {
            fltk::draw::set_font(Font::Helvetica, 13);
            fltk::draw::set_draw_color(super::muted());
            fltk::draw::draw_text2(empty, t.x() + 10, t.y(), t.w() - 20, t.h(), Align::Center | Align::Wrap);
        }
        super::picker::focus_ring(t);
        super::scroll::paint_bars(t);
    });
    t
}

/// Ask for an action name.
fn ask_name(title: &str, current: &str) -> Option<String> {
    super::ask(title, tr("actions.ask_name"), current)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Keep enough room for the editor beside the action tree; otherwise stack the same panes.
fn pane_layout(width: i32, height: i32) -> (bool, i32) {
    let stacked = width < 760;
    if !stacked {
        let actions = (width * 38 / 100).clamp(280, 340);
        return (false, actions);
    }
    // The page has 8px margins and an 8px gap between panes.
    let available = height.saturating_sub(24).max(0);
    let actions = if available < 480 {
        available / 3
    } else {
        (available / 3).clamp(160, 220)
    };
    (true, actions)
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
        .with_label(tr("actions.heading"))
        .with_align(Align::Left | Align::Inside)
        .ellipsis();
    act_head.fixed(&t, 60);
    let mut search_actions = Input::default();
    search_actions.set_tooltip(tr("actions.filter_actions"));
    super::placeholder(&mut search_actions, tr("actions.search_actions"));
    act_head.end();
    left.fixed(&act_head, ROW);
    let actions_tree = tree(tr("actions.empty_actions"));
    let main = super::grid(
        &mut left,
        &[
            tr("btn.add"),
            tr("btn.edit"),
            tr("btn.duplicate"),
            tr("btn.delete"),
            tr("btn.test"),
        ],
        1,
    );
    let files = super::grid(
        &mut left,
        &[
            tr("btn.import"),
            tr("btn.export"),
            tr("btn.templates"),
            tr("btn.add_group"),
        ],
        1,
    );
    // The group's own buttons, only while a group is selected.
    let (group_row, group) = super::grid_group(
        &mut left,
        &[
            tr("btn.group_variables"),
            tr("btn.set_group_icon"),
            tr("btn.clear_group_icon"),
            tr("btn.rename_group"),
            tr("btn.delete_group"),
        ],
        1,
    );
    let mut group_row = group_row;
    group_row.hide();
    let mut all = main;
    all.extend(files);
    all.extend(group);
    // Back in the order the handlers below index them: ..., import, export, templates, variables,
    // set icon, clear icon, add group, rename group, delete group.
    let all = [0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 8, 12, 13]
        .iter()
        .map(|&i| all[i].clone())
        .collect::<Vec<_>>();
    let (lb, lb2, lb3) = (all[0..3].to_vec(), all[3..5].to_vec(), all[5..].to_vec());
    left.end();
    row.fixed(&left, 340);
    let mut right = Flex::default().column();
    right.set_pad(4);
    let mut title = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    title.set_label_font(Font::HelveticaBold);
    right.fixed(&title, ROW - 2);
    let t = Frame::default()
        .with_label(tr("actions.triggers_heading"))
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
            tr("btn.help"),
        ],
    );
    let mut sub_head = Flex::default().row();
    sub_head.set_pad(4);
    let t = Frame::default()
        .with_label(tr("actions.subactions_heading"))
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
        &[
            tr("btn.add"),
            tr("btn.edit"),
            tr("btn.on_off"),
            tr("btn.delete"),
            tr("btn.help"),
        ],
    );

    let mut detail_title = Frame::default().with_align(Align::Left | Align::Inside).ellipsis();
    detail_title.set_label_font(Font::HelveticaBold);
    detail_title.set_label_color(Color::Foreground);
    right.fixed(&detail_title, 18);
    let mut detail_desc = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap);
    detail_desc.set_label_color(super::muted());
    right.fixed(&detail_desc, 38);

    right.end();
    row.end();

    let mut place = {
        let (left, mut right, triggers, description) =
            (left.clone(), right.clone(), triggers.clone(), detail_desc.clone());
        move |row: &mut Flex, width: i32, height: i32| {
            let (stacked, actions_size) = pane_layout(width, height);
            row.set_type(if stacked { FlexType::Column } else { FlexType::Row });
            row.fixed(&left, actions_size);
            right.fixed(&*triggers, if stacked { 56 } else { 120 });
            right.fixed(&description, if stacked { 32 } else { 38 });
            // fixed() marks the Flex layouts dirty. Their next draw lays out the existing widgets;
            // calling row.layout() here would re-enter this resize callback.
            row.redraw();
        }
    };
    let (width, height) = (row.w(), row.h());
    place(&mut row, width, height);
    row.resize_callback(move |row, _, _, width, height| place(row, width, height));

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
        group_pics: Vec::new(),
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
    on(
        &tb[5],
        Box::new(|m| {
            let cfg = m.cfg.borrow();
            let code = m
                .selected
                .as_ref()
                .and_then(|id| cfg.actions.actions.iter().find(|a| &a.id == id))
                .and_then(|a| a.triggers.get((m.triggers.value() - 1) as usize))
                .map(edit::kind);
            drop(cfg);
            if let Some(code) = code {
                dialogs::reference("trigger", code as u32);
            }
        }),
    );
    on(
        &sb[4],
        Box::new(|m| {
            let Some(path) = m.selected_sub() else { return };
            let cfg = m.cfg.borrow();
            let code = m
                .selected
                .as_ref()
                .and_then(|id| cfg.actions.actions.iter().find(|a| &a.id == id))
                .and_then(|a| edit::get(&a.sub_actions, &path))
                .map(edit::kind);
            drop(cfg);
            if let Some(code) = code {
                dialogs::reference("subaction", code as u32);
            }
        }),
    );

    // Left: the actions.
    let mut t = actions_tree.clone();
    t.set_callback({
        let me = me.clone();
        let (left, mut group_row) = (left.clone(), group_row.clone());
        move |t| {
            let Some(item) = t.callback_item() else { return };
            if t.callback_reason() != TreeReason::Selected {
                return;
            }
            let Ok(mut m) = me.try_borrow_mut() else { return };
            let is_group = matches!(m.node(&item), Some(Node::Group(_)));
            if is_group != group_row.visible() {
                if is_group {
                    group_row.show();
                } else {
                    group_row.hide();
                }
                left.layout();
            }
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
    {
        let me2 = me.clone();
        let mut b = lb3[3].clone();
        b.set_callback(move |_| {
            let (cfg, group) = {
                let m = me2.borrow();
                (m.cfg.clone(), m.current_group())
            };
            if group.is_empty() {
                dialogs::alert(tr("actions.select_group"));
                return;
            }
            super::groups::settings(&cfg, &group);
        });
    }
    on(
        &lb3[4],
        Box::new(|m| {
            let group = m.current_group();
            if group.is_empty() {
                dialogs::alert(tr("actions.select_group"));
                return;
            }
            let mut chooser = fltk::dialog::NativeFileChooser::new(fltk::dialog::NativeFileChooserType::BrowseFile);
            chooser.set_filter("PNG and JPEG\t*.{png,jpg,jpeg}");
            chooser.show();
            let path = chooser.filename();
            if path.as_os_str().is_empty() {
                return;
            }
            let result = std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|b| crate::group_icons::encode(&b).map_err(str::to_owned));
            match result {
                Ok(icon) => m.edit(|c| {
                    c.action_group_icons.insert(group, icon);
                }),
                Err(e) => {
                    let message = match e.as_str() {
                        "Image is larger than 1 MB" => tr("actions.icon_too_large"),
                        "Choose a PNG or JPEG image" => tr("actions.icon_format"),
                        _ => tr("actions.icon_read_error"),
                    };
                    dialogs::alert(message);
                }
            }
        }),
    );
    on(
        &lb3[5],
        Box::new(|m| {
            let group = m.current_group();
            if group.is_empty() {
                dialogs::alert(tr("actions.select_group"));
                return;
            }
            m.edit(|c| {
                c.action_group_icons.remove(&group);
            });
        }),
    );
    on(
        &lb3[6],
        Box::new(|m| {
            if let Some(name) = ask_name(tr("btn.add_group"), "") {
                if !name.is_empty() {
                    m.edit(|c| {
                        if !c.actions.groups.iter().any(|v| v.as_str() == Some(&name)) {
                            c.actions.groups.push(name.into());
                        }
                    });
                }
            }
        }),
    );
    on(
        &lb3[7],
        Box::new(|m| {
            let old = m.current_group();
            if old.is_empty() {
                dialogs::alert(tr("actions.select_group"));
                return;
            }
            if let Some(name) = ask_name(tr("btn.rename_group"), &old) {
                if name.is_empty() || name == old {
                    return;
                }
                if m.cfg.borrow().actions.groups.iter().any(|v| v.as_str() == Some(&name))
                    || m.cfg.borrow().actions.actions.iter().any(|a| a.group == name)
                {
                    dialogs::alert(tr("actions.group_exists"));
                    return;
                }
                m.edit(|c| {
                    groups::rename(c, &old, &name);
                });
            }
        }),
    );
    on(
        &lb3[8],
        Box::new(|m| {
            let group = m.current_group();
            if group.is_empty() {
                dialogs::alert(tr("actions.select_group"));
                return;
            }
            if !dialogs::choice(
                &trf("actions.delete_group_confirm", &[("name", &group)]),
                tr("dialogs.cancel"),
                tr("btn.delete"),
            ) {
                return;
            }
            m.edit(|c| groups::remove(c, &group));
        }),
    );
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
                Err(e) => super::dialogs::error(&plain(&e)),
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
            let mut added = Ok(());
            m.edit(|c| added = made.add_to(c));
            if let Err(e) = added {
                dialogs::error(&e);
                return;
            }
            if timers {
                super::tell(&m.cmds, |e| e.timers());
            }
            super::changed();
            crate::say(trf(
                "actions.added_from_template",
                &[("name", t.title()), ("items", &names.join(", "))],
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
            let groups = crate::groups::names(&m.cfg.borrow());
            let cfg = m.cfg.clone();
            dialogs::export(&list, &groups, m.selected.as_deref(), move |ids, groups, meta| {
                crate::model::write_export(&edit::export_with(&cfg.borrow(), ids, groups, meta))
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
        Event::Push if super::right_button() => {
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
        Event::KeyDown if super::menu_key() => {
            super::context_menu(&menu);
            true
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
fn edit_obj(m: &Actions, title: &str, fields: Option<&[forms::F]>, o: &Obj, hint: &str, trigger: bool) -> Option<Obj> {
    let draft = if let Some(fields) = fields {
        let mut lists = Lists::new(&m.cfg.borrow());
        lists.cmds = Some(m.cmds.clone());
        if let Some(action) = m
            .selected
            .as_ref()
            .and_then(|id| m.cfg.borrow().actions.actions.iter().find(|a| &a.id == id).cloned())
        {
            let mut values = std::collections::BTreeMap::new();
            for trigger in &action.triggers {
                for key in crate::docs::argument_names("trigger", edit::kind(trigger) as u32) {
                    values.insert(key.to_string(), value_label(key));
                }
            }
            for key in crate::groups::values(&m.cfg.borrow(), &action.group).keys() {
                values.insert(format!("group.{key}"), trf("automation.group_value", &[("name", key)]));
            }
            let current = s(o, "id");
            let mut before = true;
            edit::walk(&action.sub_actions, &mut |sub| {
                if s(sub, "id") == current {
                    before = false;
                }
                if before {
                    for key in crate::docs::argument_names("subaction", edit::kind(sub) as u32) {
                        values.insert(key.into(), value_label(key));
                    }
                    if let Some(key) = sub
                        .get("variableName")
                        .and_then(Value::as_str)
                        .filter(|key| !key.is_empty())
                    {
                        values.insert(key.into(), value_label(key));
                    }
                }
            });
            lists.values = values
                .into_iter()
                .map(|(key, label)| (label, format!("%{key}%")))
                .collect();
        }
        match forms::form_reference(title, fields, o, &lists, if trigger { "trigger" } else { "subaction" }) {
            Done::Ok(x) => return Some(x),
            Done::Cancel => return None,
            Done::Json(draft) => draft,
        }
    } else {
        o.clone()
    };
    let k = edit::kind(o);
    let help_ref = Some((if trigger { "trigger" } else { "subaction" }, k as u32));
    let new = dialogs::json_editor_reference(title, hint, &Value::Object(draft), help_ref, |v| {
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

fn value_label(key: &str) -> String {
    let friendly = match key {
        "user" | "userName" => Some(tr("automation.sender_name")),
        "message" => Some(tr("automation.message_value")),
        "rawInput" => Some(tr("automation.command_input")),
        "giftName" => Some(tr("automation.gift_name")),
        "repeatCount" => Some(tr("automation.gift_quantity")),
        "coins" => Some(tr("automation.gift_coins")),
        "diamondCount" => Some(tr("automation.unit_coins")),
        _ => None,
    };
    match friendly {
        Some(label) => format!("{label} (%{key}%)"),
        None => format!("%{key}%"),
    }
}

pub(super) fn trigger_title(k: u64) -> String {
    match k {
        706 => tr("automation.started").to_string(),
        707 => tr("automation.exiting").to_string(),
        _ if name(TRIGGERS, k as u32).is_some() => {
            let raw = name(TRIGGERS, k as u32).unwrap();
            let cat = super::picker::trigger_categorize(raw);
            let platform = cat.split('/').next().unwrap_or(&cat);
            let clean = super::picker::trigger_item_title(k as u32, raw);
            format!("{platform}: {clean}")
        }
        _ => tr("actions.unknown").to_string(),
    }
}

pub(super) fn sub_title(k: u64) -> &'static str {
    if let Some(title) = super::picker::spotify_player_title(k as u32) {
        return title;
    }
    if k == 99802 {
        return tr("spotify.action_name");
    }
    if k == 99803 {
        return tr("countdown.action_name");
    }
    if k == 99880 {
        return tr("overlay.action_name");
    }
    SUB_ACTION_INFO
        .binary_search_by_key(&(k as u32), |x| x.0)
        .map_or("Sub-action", |i| SUB_ACTION_INFO[i].1)
}

/// A new trigger or sub-action with fields opens its form first; cancelling it adds nothing.
fn fill_new(m: &Actions, o: Obj, trigger: bool) -> Option<Obj> {
    let k = edit::kind(&o);
    let title = if trigger {
        trigger_title(k)
    } else {
        sub_title(k).to_string()
    };
    match fields_of(m, &o, trigger) {
        Some(f) => edit_obj(m, &title, Some(f), &o, "", trigger),
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
                reference: None,
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
    let hint = crate::docs::summary("trigger", k as u32).unwrap_or(tr("actions.trigger_hint"));
    let fields = fields_of(&m, &t, true);
    let fields = fields.or_else(|| crate::docs::available("trigger", k as u32).then_some(&[][..]));
    if let Some(mut new) = edit_obj(&m, &title, fields, &t, hint, true) {
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
    if let Some(mut new) = edit_obj(&m, title, fields_of(&m, &o, false), &o, &hint, false) {
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
    let (mut win, answer) = dialogs::window(560, 370, tr("actions.template_title"));
    let mut col = Flex::new(10, 0, 540, 360, None).column();
    col.set_pad(6);
    let mut list = Rows::new("");
    for t in TEMPLATES {
        list.add(if t.name == "Song requests (Spotify)" {
            tr("spotify.template_name")
        } else {
            t.title()
        });
    }
    list.select(1);
    let mut about = Frame::default().with_align(Align::Left | Align::Inside | Align::Wrap | Align::Top);
    about.set_label(TEMPLATES[0].description());
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
    list.set_callback({
        let mut about = about.clone();
        let a = answer.clone();
        move |l| {
            if let Some(t) = TEMPLATES.get((l.value() - 1).max(0) as usize) {
                let text = if t.name == "Song requests (Spotify)" {
                    tr("spotify.template_about")
                } else {
                    t.description()
                };
                about.set_label(&text.replace('@', "@@"));
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
    let picked = dialogs::run(&mut win, &answer, || true) && list.value() > 0;
    picked.then(|| &TEMPLATES[(list.value() - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::pane_layout;

    #[test]
    fn action_editor_stacks_before_the_two_panes_become_cramped() {
        assert_eq!(pane_layout(759, 654), (true, 210));
        assert_eq!(pane_layout(760, 654), (false, 288));
        assert_eq!(pane_layout(860, 654), (false, 326));
        assert_eq!(pane_layout(924, 754), (false, 340));
        assert_eq!(pane_layout(364, 654), (true, 210));
    }

    #[test]
    fn stacked_action_pane_reserves_the_editor_and_bounds_its_height() {
        for height in [-100, 0, 20, 100, 354, 654, 900, 1600] {
            let (stacked, actions) = pane_layout(364, height);
            let available = height.saturating_sub(24).max(0);
            assert!(stacked);
            assert!((0..=available).contains(&actions));
            assert!(actions <= 220);
            assert!(available - actions >= actions);
        }
        assert_eq!(pane_layout(364, 0), (true, 0));
        assert_eq!(pane_layout(364, 1600), (true, 220));
    }
}
