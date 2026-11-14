//! A searchable tree with a button bar and a right-click menu, drawn from any model of nodes and
//! buttons. The core decides what the rows and buttons mean; a page decides what a press does.
use crate::i18n::tr;
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
    list::ListItem,
    menu::PopupMenuItem,
    tree::{tree, TreeItem, TreeState},
    ActiveTheme, Disableable, Icon, Sizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value;
use std::collections::HashSet;

/// A row of the tree as a page needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub detail: Value,
    /// The questions this row asks instead of a button's own, by the button's id.
    pub confirm: Value,
}

pub enum PaneEvent {
    /// The selection changed (None: nothing is selected now).
    Selected(Option<Node>),
    /// A double click.
    Activate(Node),
    /// A button or menu entry, with the row it is about.
    Press { spec: Value, node: Option<Node> },
    /// The search text changed.
    Filter(String),
}
impl EventEmitter<PaneEvent> for TreePane {}

#[derive(Clone, Copy, Default)]
pub struct Options {
    /// Folders start closed (a picker); the others start open.
    pub closed: bool,
    /// Buttons show only their icons.
    pub compact: bool,
    /// The pane's width when known: buttons fold to their icons when a label no longer fits (1.6.0: under 80).
    pub width: f32,
    /// Nothing is open: every button is off.
    pub idle: bool,
    /// The first row of buttons is always icons.
    pub icons_first: bool,
    /// The title sits left of the search box.
    pub header: bool,
    /// Expand all and Collapse all beside the search box (a picker).
    pub fold_buttons: bool,
}

pub struct TreePane {
    pub model: Value,
    state: Entity<TreeState>,
    search: Option<Entity<InputState>>,
    external_filter: String,
    selected: Option<Node>,
    /// Folders the user turned away from their starting state.
    toggled: HashSet<String>,
    options: Options,
    id: SharedString,
    _subscriptions: Vec<Subscription>,
}

fn find<'a>(nodes: &'a Value, id: &str) -> Option<&'a Value> {
    for node in nodes.as_array().into_iter().flatten() {
        if node["id"] == id {
            return Some(node);
        }
        if let Some(found) = find(&node["children"], id) {
            return Some(found);
        }
    }
    None
}

fn compact_of(options: Options, row: usize, per_row: f32, count: f32) -> bool {
    if options.icons_first && row == 0 {
        return true;
    }
    if options.width > 0. {
        return (options.width - 8. * (count - 1.)) / count < 80.;
    }
    let _ = per_row;
    options.compact
}

fn has_folders(nodes: &Value) -> bool {
    nodes
        .as_array()
        .into_iter()
        .flatten()
        .any(|node| node["children"].as_array().is_some_and(|kids| !kids.is_empty()) || node["kind"] == "group")
}

fn node_of(value: &Value) -> Node {
    Node {
        id: value["id"].as_str().unwrap_or_default().to_owned(),
        kind: value["kind"].as_str().unwrap_or_default().to_owned(),
        label: value["label"].as_str().unwrap_or_default().to_owned(),
        detail: value["detail"].clone(),
        confirm: value["confirm"].clone(),
    }
}

/// Whether a button can act on this row.
pub fn enabled(spec: &Value, node: Option<&Node>) -> bool {
    let needs = spec["needs"].as_array().map(Vec::as_slice).unwrap_or_default();
    needs.is_empty() || node.is_some_and(|node| needs.iter().any(|need| *need == node.kind.as_str()))
}

impl TreePane {
    pub fn new(
        id: impl Into<SharedString>,
        model: Value,
        searchable: bool,
        options: Options,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| TreeState::new(cx));
        let search = searchable.then(|| {
            let hint = model["searchHint"].as_str().unwrap_or_default().to_owned();
            cx.new(|cx| InputState::new(window, cx).placeholder(hint))
        });
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&state, |pane, _, event, cx| {
            use gpui_kit::base::TreeEvent;
            if !pane.query(cx).is_empty() {
                return;
            }
            let (id, open) = match event {
                TreeEvent::Expanded(id) => (id.to_string(), true),
                TreeEvent::Collapsed(id) => (id.to_string(), false),
            };
            // Turned away from the starting state, or back to it.
            if open == !pane.options.closed {
                pane.toggled.remove(&id);
            } else {
                pane.toggled.insert(id);
            }
        }));
        if let Some(search) = &search {
            subscriptions.push(cx.subscribe(search, |pane, search, event, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = search.read(cx).value().to_string();
                    pane.rebuild(cx);
                    cx.emit(PaneEvent::Filter(query));
                }
            }));
        }
        let mut pane = Self {
            model,
            state,
            search,
            external_filter: String::new(),
            selected: None,
            toggled: HashSet::new(),
            options,
            id: id.into(),
            _subscriptions: subscriptions,
        };
        pane.rebuild(cx);
        pane
    }

    pub fn selected(&self) -> Option<&Node> {
        self.selected.as_ref()
    }
    pub fn set_options(&mut self, options: Options, cx: &mut Context<Self>) {
        let (closed, now) = (self.options.closed, options);
        self.options = Options { closed, ..now };
        cx.notify();
    }
    fn query(&self, cx: &App) -> String {
        match &self.search {
            Some(search) => search.read(cx).value().trim().to_lowercase(),
            None => self.external_filter.trim().to_lowercase(),
        }
    }
    /// A search typed elsewhere filters this tree too.
    pub fn set_filter(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.search.is_none() && self.external_filter != query {
            self.external_filter = query.to_owned();
            self.rebuild(cx);
        }
    }
    /// Open (or close) every folder.
    pub fn open_all(&mut self, open: bool, cx: &mut Context<Self>) {
        fn folders(nodes: &Value, out: &mut HashSet<String>) {
            for node in nodes.as_array().into_iter().flatten() {
                if node["children"].as_array().is_some_and(|kids| !kids.is_empty()) {
                    if let Some(id) = node["id"].as_str() {
                        out.insert(id.to_owned());
                    }
                    folders(&node["children"], out);
                }
            }
        }
        self.toggled.clear();
        // Folders start closed in a picker: opening all turns every one away from that.
        if open == self.options.closed {
            folders(&self.model["nodes"], &mut self.toggled);
        }
        self.rebuild(cx);
    }
    pub fn set_model(&mut self, model: Value, cx: &mut Context<Self>) {
        self.model = model;
        self.rebuild(cx);
    }

    fn items(&self, nodes: &Value, query: &str) -> Vec<TreeItem> {
        nodes
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|node| {
                let id = node["id"].as_str()?;
                let label = node["label"].as_str().unwrap_or_default();
                let own = query.is_empty()
                    || label.to_lowercase().contains(query)
                    || (node["kind"] == "item"
                        && node["detail"]["text"]
                            .as_str()
                            .is_some_and(|text| text.to_lowercase().contains(query)));
                let children = self.items(&node["children"], query);
                if !own && children.is_empty() {
                    return None;
                }
                // A folder that matches shows everything in it.
                let children = if own && !query.is_empty() {
                    self.items(&node["children"], "")
                } else {
                    children
                };
                let open = !query.is_empty() || (self.toggled.contains(id) == self.options.closed);
                Some(
                    TreeItem::new(id.to_owned(), label.to_owned())
                        .children(children)
                        .expanded(open),
                )
            })
            .collect()
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        let items = self.items(&self.model["nodes"], &query);
        let wanted = self.selected.as_ref().map(|node| node.id.clone());
        let mut found = None;
        self.state.update(cx, |state, cx| {
            state.set_items(items, cx);
            if let Some(id) = &wanted {
                let id: SharedString = id.clone().into();
                if let Some(ix) = state.index_of(&id) {
                    state.set_selected_index(Some(ix), cx);
                    found = Some(id);
                }
            }
        });
        let now = found.and_then(|id| find(&self.model["nodes"], &id)).map(node_of);
        if now != self.selected {
            self.selected = now.clone();
            cx.emit(PaneEvent::Selected(now));
        }
        cx.notify();
    }

    /// Select a row by its id, opening the folders above it.
    pub fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(value) = find(&self.model["nodes"], id) else {
            return;
        };
        let node = node_of(value);
        let wanted: SharedString = id.to_owned().into();
        self.state.update(cx, |state, cx| {
            state.reveal_item(&wanted, gpui_kit::ScrollStrategy::Nearest, cx);
            let ix = state.index_of(&wanted);
            state.set_selected_index(ix, cx);
        });
        if self.selected.as_ref() != Some(&node) {
            self.selected = Some(node.clone());
            cx.emit(PaneEvent::Selected(Some(node)));
        }
        cx.notify();
    }

    fn clicked(&mut self, id: &str, clicks: usize, cx: &mut Context<Self>) {
        let Some(value) = find(&self.model["nodes"], id) else {
            return;
        };
        let node = node_of(value);
        if self.selected.as_ref() != Some(&node) {
            self.selected = Some(node.clone());
            cx.emit(PaneEvent::Selected(Some(node.clone())));
        }
        if clicks >= 2 && value["kind"] != "group" && value["kind"] != "category" {
            cx.emit(PaneEvent::Activate(node));
        }
        cx.notify();
    }

    pub fn press(&mut self, spec: Value, node: Option<Node>, cx: &mut Context<Self>) {
        if enabled(&spec, node.as_ref()) {
            cx.emit(PaneEvent::Press { spec, node });
        }
    }

    fn buttons(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let specs: Vec<Value> = self.model["buttons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|spec| spec["menuOnly"] != true)
            .filter(|spec| spec["hidden"] != true || enabled(spec, self.selected.as_ref()))
            .cloned()
            .collect();
        if specs.is_empty() {
            return None;
        }
        let per_row = self.model["perRow"].as_u64().unwrap_or(4).max(1) as usize;
        let mut column = div().flex().flex_col().gap(px(4.)).w_full().flex_shrink_0();
        for (row, chunk) in specs.chunks(per_row).enumerate() {
            let mut line = div().flex().gap_2().w_full();
            let compact = compact_of(self.options, row, per_row as f32, chunk.len() as f32);
            for (at, spec) in chunk.iter().enumerate() {
                let label = spec["label"].as_str().unwrap_or_default().to_owned();
                let on = !self.options.idle && enabled(spec, self.selected.as_ref());
                let spec = spec.clone();
                line = line.child(
                    Button::new(SharedString::from(format!(
                        "{}-{}-{}",
                        self.id,
                        spec["id"].as_str().unwrap_or_default(),
                        row * per_row + at
                    )))
                    .small()
                    .h(px(28.))
                    .flex_1()
                    .min_w_0()
                    .icon(Icon::empty().path(format!("icons/{}.svg", spec["icon"].as_str().unwrap_or("circle"))))
                    .when(!compact, |button| button.label(label.clone()))
                    .accessibility_label(match self.model["title"].as_str() {
                        Some(title) if !title.is_empty() => format!("{title}: {label}"),
                        _ => label.clone(),
                    })
                    .tooltip(spec["tooltip"].as_str().map_or(label.clone(), str::to_owned))
                    .disabled(!on)
                    .on_click(cx.listener(move |pane, _, _, cx| {
                        let node = pane.selected.clone();
                        pane.press(spec.clone(), node, cx)
                    })),
                );
            }
            column = column.child(line);
        }
        Some(column.into_any_element())
    }
}

impl Render for TreePane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        // A selection made with the keyboard.
        let current = self.state.read(cx).selected_item().map(|item| item.id.to_string());
        if current.as_deref() != self.selected.as_ref().map(|node| node.id.as_str()) {
            if let Some(id) = current {
                let owner = cx.entity();
                window.defer(cx, move |_, cx| owner.update(cx, |pane, cx| pane.clicked(&id, 1, cx)));
            }
        }
        let rows = {
            let weak = weak.clone();
            let id = self.id.clone();
            let has_folders = has_folders(&self.model["nodes"]);
            move |ix: usize, entry: &gpui_kit::base::TreeEntry, selected: bool, _: &mut Window, cx: &mut App| {
                let item = entry.item();
                let node_id = item.id.to_string();
                let value = weak
                    .upgrade()
                    .and_then(|pane| find(&pane.read(cx).model["nodes"], &node_id).cloned())
                    .unwrap_or(Value::Null);
                let muted = value["muted"] == true;
                let icon = value["icon"].as_str().unwrap_or("circle").to_owned();
                let color = if muted {
                    cx.theme().muted_foreground
                } else {
                    cx.theme().foreground
                };
                let mut label = div().flex_1().min_w_0().truncate().child(item.label.to_string());
                if value["italic"] == true {
                    label = label.italic();
                }
                if value["kind"] == "group" || value["kind"] == "category" {
                    label = label.font_weight(FontWeight::BOLD);
                }
                let folder = entry.is_folder();
                let weak = weak.clone();
                ListItem::new(SharedString::from(format!("{id}-row-{ix}")))
                    .selected(selected)
                    .accessibility_label(item.label.to_string())
                    .px_1()
                    .on_click(move |event, _, cx| {
                        let clicks = event.click_count();
                        let node_id = node_id.clone();
                        let _ = weak.update(cx, |pane, cx| pane.clicked(&node_id, clicks, cx));
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("{id}-line-{ix}")))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .w_full()
                            // What the row is, on hover (an action: what starts it and what it runs, or full label).
                            .when_some(
                                value["tip"]
                                    .as_str()
                                    .map(|tip| SharedString::from(tip.to_owned()))
                                    .or_else(|| {
                                        let s = item.label.to_string();
                                        (!s.is_empty()).then(|| SharedString::from(s))
                                    }),
                                |line, tip| {
                                    line.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                    })
                                },
                            )
                            .text_size(px(if value["kind"] == "action" { 12.5 } else { 13.5 }))
                            .pl(px(entry.depth() as f32 * 14.))
                            .text_color(color)
                            .child(
                                div()
                                    .w(px(14.))
                                    .flex_shrink_0()
                                    .when(!has_folders, |slot| slot.w(px(0.)))
                                    .when(folder, |slot| {
                                        slot.child(
                                            Icon::empty()
                                                .path(if entry.is_expanded() {
                                                    "icons/chevron-down.svg"
                                                } else {
                                                    "icons/chevron-right.svg"
                                                })
                                                .size_3(),
                                        )
                                    }),
                            )
                            .child(match value["iconFile"].as_str() {
                                // A platform's or app's logo, in its own colours.
                                None if value["logo"].is_string() && !muted => {
                                    img(value["logo"].as_str().unwrap_or_default().to_owned())
                                        .id(SharedString::from(format!("{id}-logo-{ix}")))
                                        .size_4()
                                        .flex_shrink_0()
                                        .into_any_element()
                                }
                                // A group's own picture replaces the folder.
                                Some(file) => img(std::path::PathBuf::from(file))
                                    .id(SharedString::from(format!("{id}-icon-{ix}")))
                                    .size_4()
                                    .rounded_sm()
                                    .flex_shrink_0()
                                    .object_fit(ObjectFit::Cover)
                                    .into_any_element(),
                                None => Icon::empty()
                                    .path(format!("icons/{icon}.svg"))
                                    .small()
                                    .into_any_element(),
                            })
                            .child(label)
                            // A quiet count at the right (an action's sub-actions).
                            .when_some(value["note"]["text"].as_str().map(str::to_owned), |row, text| {
                                row.child(
                                    div()
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.))
                                        .text_size(px(11.5))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            Icon::empty()
                                                .path(format!(
                                                    "icons/{}.svg",
                                                    value["note"]["icon"].as_str().unwrap_or("circle")
                                                ))
                                                .size_3(),
                                        )
                                        .child(text),
                                )
                            })
                            .when((value["kind"] == "trigger" && muted) || value["off"] == true, |row| {
                                row.pr(px(16.)).child(Icon::empty().path("icons/power-off.svg").small())
                            }),
                    )
            }
        };
        let menu_specs: Vec<Value> = self.model["buttons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|spec| spec["hidden"] != true || spec["needs"].as_array().is_some_and(|needs| !needs.is_empty()))
            .cloned()
            .collect();
        let menu_weak = weak.clone();
        let rows_model = self.model.clone();
        let list = tree(&self.state, rows).context_menu(move |_, entry, mut menu, _, cx| {
            let Some(value) = find(&rows_model["nodes"], &entry.item().id).cloned() else {
                return menu;
            };
            let node = node_of(&value);
            let _ = cx;
            for spec in &menu_specs {
                if spec["hidden"] == true && !enabled(spec, Some(&node)) {
                    continue;
                }
                let owner = menu_weak.clone();
                let (pressed, row) = (spec.clone(), node.clone());
                menu = menu.item(
                    PopupMenuItem::new(spec["label"].as_str().unwrap_or_default().to_owned())
                        .icon(Icon::empty().path(format!("icons/{}.svg", spec["icon"].as_str().unwrap_or("circle"))))
                        .disabled(!enabled(spec, Some(&node)))
                        .on_click(move |_, _, cx| {
                            let (pressed, row) = (pressed.clone(), row.clone());
                            let _ = owner.update(cx, |pane, cx| pane.press(pressed, Some(row), cx));
                        }),
                );
            }
            menu
        });
        let empty = self.model["nodes"].as_array().is_none_or(Vec::is_empty);
        // Keys on the selected row (the search box keeps its own): Enter edits, Delete deletes, Space switches.
        let mut body = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .min_h_0()
            .on_key_down(cx.listener(|pane, event: &KeyDownEvent, window, cx| {
                let id = match event.keystroke.key.as_str() {
                    "enter" => "edit",
                    "delete" | "backspace" => "delete",
                    "space" => "toggle",
                    _ => return,
                };
                let typing = pane
                    .search
                    .as_ref()
                    .is_some_and(|search| search.read(cx).focus_handle(cx).contains_focused(window, cx));
                let Some(node) = pane.selected.clone().filter(|_| !typing) else {
                    return;
                };
                let spec = pane.model["buttons"]
                    .as_array()
                    .and_then(|specs| specs.iter().find(|spec| spec["id"] == id))
                    .cloned();
                if let Some(spec) = spec {
                    pane.press(spec, Some(node), cx);
                    cx.stop_propagation();
                }
            }));
        if let Some(search) = &self.search {
            // In a column the box keeps its own height; beside a title it takes the rest of the row.
            let field = div()
                .when(self.options.header, |field| field.flex_1())
                .when(!self.options.header, |field| field.w_full().flex_shrink_0())
                .min_w_0()
                .h(px(26.))
                .child(
                    Input::new(search)
                        .small()
                        .prefix(
                            Icon::empty()
                                .path("icons/search.svg")
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .min_h(px(26.))
                        .bg(cx.theme().table)
                        .text_size(px(13.))
                        .aria_label(self.model["searchHint"].as_str().unwrap_or_default().to_owned()),
                );
            let field = if self.options.fold_buttons {
                let fold = |id: &'static str, icon: &str, tip: &'static str, open: bool| {
                    Button::new(id)
                        .small()
                        .h(px(26.))
                        .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                        .tooltip(tr(tip))
                        .accessibility_label(tr(tip))
                        .on_click(cx.listener(move |pane, _, _, cx| pane.open_all(open, cx)))
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .w_full()
                    .flex_shrink_0()
                    .child(field.flex_1().w_auto())
                    .child(fold("tree-expand-all", "chevrons-up-down", "lists.expand_all", true))
                    .child(fold(
                        "tree-collapse-all",
                        "chevrons-down-up",
                        "lists.collapse_all",
                        false,
                    ))
            } else {
                field
            };
            body = body.child(if self.options.header {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .pr(px(12.))
                            .flex_shrink_0()
                            .text_color(cx.theme().foreground)
                            .child(self.model["title"].as_str().unwrap_or_default().to_owned()),
                    )
                    .child(field)
                    .into_any_element()
            } else {
                field.into_any_element()
            });
        }
        body = body.child(
            div()
                .id(SharedString::from(format!("{}-tree", self.id)))
                .flex_1()
                .min_h_0()
                .relative()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().input)
                .bg(cx.theme().table)
                .child(list)
                .when(empty, |frame| {
                    frame.child(
                        div()
                            .absolute()
                            .inset_0()
                            .p_3()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                div().w_full().min_w_0().child(
                                    if self.query(cx).is_empty() {
                                        self.model["empty"].as_str().unwrap_or_default()
                                    } else {
                                        tr("lists.no_matches")
                                    }
                                    .to_owned(),
                                ),
                            ),
                    )
                }),
        );
        if let Some(buttons) = self.buttons(cx) {
            body = body.child(buttons);
        }
        body
    }
}
