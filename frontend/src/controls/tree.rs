//! A searchable tree with a button bar and a right-click menu, drawn from any model of nodes and
//! buttons. The core decides what the rows and buttons mean; a page decides what a press does.
use crate::i18n::tr;
use crate::theme::{
    CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, LINE_BODY, LINE_CAPTION, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS,
};
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
    list::ListItem,
    menu::{ContextMenuExt, PopupMenuItem},
    switch::Switch,
    tree::{tree, TreeItem, TreeState},
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
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

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Options {
    /// Folders start closed (a picker); the others start open.
    pub closed: bool,
    /// Buttons show only their icons.
    pub compact: bool,
    /// The pane's width when known: buttons fold to their icons when a label no longer fits (under 80).
    pub width: f32,
    /// Nothing is open: every button is off.
    pub idle: bool,
    /// The first row of buttons is always icons.
    pub icons_first: bool,
    /// The title sits left of the search box.
    pub header: bool,
    /// Expand all and Collapse all beside the search box (a picker).
    pub fold_buttons: bool,
    /// The page's header carries the buttons the model marks `"header": true`, so the bar under the tree
    /// leaves them out (the Actions page: Add, Import, Templates).
    pub in_header: bool,
}

pub struct TreePane {
    pub model: Value,
    state: Entity<TreeState>,
    search: Option<Entity<InputState>>,
    external_filter: String,
    visible_empty: bool,
    selected: Option<Node>,
    /// Folders the user turned away from their starting state.
    toggled: HashSet<String>,
    options: Options,
    id: SharedString,
    header_view: Entity<HeaderButtons>,
    _subscriptions: Vec<Subscription>,
}

/// A tree's page-wide buttons, drawn in the page's header.
pub struct HeaderButtons(WeakEntity<TreePane>);
impl Render for HeaderButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self
            .0
            .upgrade()
            .and_then(|pane| pane.update(cx, |pane, cx| pane.header_buttons(cx)))
        {
            Some(buttons) => buttons,
            None => div().into_any_element(),
        }
    }
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
        return (options.width - SPACE_SM * (count - 1.)) / count < 80.;
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

fn key_spec(key: &str, buttons: &[Value], node: &Node) -> Option<Value> {
    if key == "f2" {
        return ["rename", "group_rename", "edit"].iter().find_map(|id| {
            buttons
                .iter()
                .find(|spec| spec["id"] == *id && enabled(spec, Some(node)))
                .cloned()
        });
    }
    let id = match key {
        "enter" => "edit",
        "delete" | "backspace" => "delete",
        "space" => "toggle",
        _ => return None,
    };
    buttons
        .iter()
        .find(|spec| spec["id"] == id && enabled(spec, Some(node)))
        .cloned()
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
        let header_view = {
            let weak = cx.entity().downgrade();
            cx.new(|_| HeaderButtons(weak))
        };
        let mut pane = Self {
            model,
            header_view,
            state,
            search,
            external_filter: String::new(),
            visible_empty: false,
            selected: None,
            toggled: HashSet::new(),
            options,
            id: id.into(),
            _subscriptions: subscriptions,
        };
        pane.rebuild(cx);
        pane
    }

    /// The page-wide buttons as their own view, for the page's header.
    pub fn header_view(&self) -> Entity<HeaderButtons> {
        self.header_view.clone()
    }
    pub fn selected(&self) -> Option<&Node> {
        self.selected.as_ref()
    }
    pub fn set_options(&mut self, options: Options, cx: &mut Context<Self>) {
        let (closed, now) = (self.options.closed, options);
        let options = Options { closed, ..now };
        if self.options != options {
            self.options = options;
            cx.notify();
        }
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
        self.visible_empty = items.is_empty();
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

    /// The buttons the model marks `"header": true`, in a row for the page's header (none unless the options
    /// hand them over).
    fn header_buttons(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.options.in_header {
            return None;
        }
        let specs: Vec<Value> = self.model["buttons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|spec| spec["header"] == true && spec["menuOnly"] != true)
            .cloned()
            .collect();
        if specs.is_empty() {
            return None;
        }
        let mut row = div().flex().items_center().gap(px(SPACE_SM)).flex_shrink_0();
        for spec in specs {
            let label = spec["label"].as_str().unwrap_or_default().to_owned();
            let id = spec["id"].as_str().unwrap_or_default().to_owned();
            row = row.child(
                Button::new(SharedString::from(format!("{}-header-{id}", self.id)))
                    .small()
                    .h(px(CONTROL_HEIGHT))
                    .flex_shrink_0()
                    .icon(Icon::empty().path(format!("icons/{}.svg", spec["icon"].as_str().unwrap_or("circle"))))
                    .label(label.clone())
                    .accessibility_label(match self.model["title"].as_str() {
                        Some(title) if !title.is_empty() => format!("{title}: {label}"),
                        _ => label.clone(),
                    })
                    .tooltip(spec["tooltip"].as_str().map_or(label, str::to_owned))
                    .disabled(!enabled(&spec, None))
                    .on_click(cx.listener(move |pane, _, _, cx| {
                        let node = pane.selected.clone();
                        pane.press(spec.clone(), node, cx)
                    })),
            );
        }
        Some(row.into_any_element())
    }

    /// The button bar. `fill`: the buttons share the row's width (under a tree); otherwise they take the width
    /// of their labels and wrap (the empty state).
    fn buttons(&self, fill: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let specs: Vec<Value> = self.model["buttons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|spec| spec["menuOnly"] != true)
            .filter(|spec| !(self.options.in_header && spec["header"] == true))
            .filter(|spec| !self.model["nodes"].as_array().is_some_and(Vec::is_empty) || enabled(spec, None))
            .filter(|spec| spec["hidden"] != true || enabled(spec, self.selected.as_ref()))
            .filter(|spec| {
                !self.model["nodes"].as_array().is_some_and(Vec::is_empty)
                    || matches!(spec["id"].as_str(), Some("add" | "templates" | "import"))
            })
            .cloned()
            .collect();
        if specs.is_empty() {
            return None;
        }
        let requested = self.model["perRow"].as_u64().unwrap_or(4).max(1) as usize;
        // Wrap before folding so labelled actions remain available in an ordinary tree pane.
        let per_row = if self.options.width > 0. && !self.options.compact {
            requested.min(((self.options.width + SPACE_SM) / 104.).floor().max(1.) as usize)
        } else {
            requested
        };
        let per_row = if fill { per_row } else { specs.len() };
        let mut column = div().flex().flex_col().gap(px(SPACE_SM)).w_full().flex_shrink_0();
        for (row, chunk) in specs.chunks(per_row).enumerate() {
            let mut line = div()
                .flex()
                .gap(px(SPACE_SM))
                .w_full()
                .when(!fill, |line| line.flex_wrap());
            let compact = fill && compact_of(self.options, row, per_row as f32, chunk.len() as f32);
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
                    .h(px(CONTROL_HEIGHT))
                    .when(fill, |button| button.flex_1().min_w_0())
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
                let toggle_owner = weak.clone();
                ListItem::new(SharedString::from(format!("{id}-row-{ix}")))
                    .selected(selected)
                    .accessibility_label(item.label.to_string())
                    .px(px(SPACE_MD))
                    .py(px(0.))
                    .h(px(CONTROL_HEIGHT))
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
                            .gap(px(SPACE_SM))
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
                            .text_size(px(FONT_BODY))
                            .line_height(px(LINE_BODY))
                            .pl(px(entry.depth() as f32 * SPACE_LG))
                            .text_color(color)
                            .when(value["toggle"].is_object(), |line| {
                                let weak = toggle_owner.clone();
                                let node_id = item.id.to_string();
                                let toggle = value["toggle"].clone();
                                line.child(
                                    Switch::new(SharedString::from(format!("{id}-toggle-{ix}")))
                                        .small()
                                        .checked(toggle["checked"] == true)
                                        .accessibility_label(toggle["label"].as_str().unwrap_or_default().to_owned())
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            let _ = weak.update(cx, |pane, cx| {
                                                let spec = pane.model["buttons"]
                                                    .as_array()
                                                    .and_then(|buttons| {
                                                        buttons.iter().find(|spec| spec["id"] == toggle["action"])
                                                    })
                                                    .cloned();
                                                let node = find(&pane.model["nodes"], &node_id).map(node_of);
                                                if let Some(spec) = spec {
                                                    pane.press(spec, node, cx);
                                                }
                                            });
                                        }),
                                )
                            })
                            .child(
                                div()
                                    .w(px(SPACE_LG))
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
                                        .gap(px(SPACE_XS))
                                        .text_size(px(FONT_CAPTION))
                                        .line_height(px(LINE_CAPTION))
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
                                row.pr(px(SPACE_LG))
                                    .child(Icon::empty().path("icons/power-off.svg").small())
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
        // Keys on the selected row (the search box keeps its own): Enter/F2 edit/rename, Delete deletes, Space switches.
        let mut body = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(SPACE_SM))
            .min_h_0()
            .on_key_down(cx.listener(|pane, event: &KeyDownEvent, window, cx| {
                if event.keystroke.modifiers.control
                    || event.keystroke.modifiers.alt
                    || event.keystroke.modifiers.platform
                    || window.has_active_dialog(cx)
                {
                    return;
                }
                let typing = pane
                    .search
                    .as_ref()
                    .is_some_and(|search| search.read(cx).focus_handle(cx).contains_focused(window, cx));
                let Some(node) = pane.selected.clone().filter(|_| !typing) else {
                    return;
                };
                let buttons = pane.model["buttons"].as_array().map(Vec::as_slice).unwrap_or_default();
                if let Some(spec) = key_spec(event.keystroke.key.as_str(), buttons, &node) {
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
                .h(px(CONTROL_HEIGHT))
                .child(
                    Input::new(search)
                        .small()
                        .prefix(
                            Icon::empty()
                                .path("icons/search.svg")
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .min_h(px(CONTROL_HEIGHT))
                        .bg(cx.theme().table)
                        .text_size(px(FONT_BODY))
                        .aria_label(self.model["searchHint"].as_str().unwrap_or_default().to_owned()),
                );
            let field = if self.options.fold_buttons {
                let fold = |id: &'static str, icon: &str, tip: &'static str, open: bool| {
                    Button::new(id)
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                        .tooltip(tr(tip))
                        .accessibility_label(tr(tip))
                        .on_click(cx.listener(move |pane, _, _, cx| pane.open_all(open, cx)))
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_SM))
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
                    .gap(px(SPACE_SM))
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .pr(px(SPACE_MD))
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
        if empty && self.query(cx).is_empty() {
            let specs = self.model["buttons"].as_array().cloned().unwrap_or_default();
            let owner = weak.clone();
            let mut state = div()
                .id("empty-tree-state")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(SPACE_MD))
                .child(super::notice::info(
                    self.model["empty"].as_str().unwrap_or_default(),
                    cx,
                ));
            if let Some(buttons) = self.buttons(false, cx) {
                state = state.child(buttons);
            }
            return body.child(state.context_menu(move |mut menu, _, _| {
                for spec in &specs {
                    let pressed = spec.clone();
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(spec["label"].as_str().unwrap_or_default().to_owned())
                            .disabled(!enabled(spec, None))
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |pane, cx| pane.press(pressed.clone(), None, cx));
                            }),
                    );
                }
                menu
            }));
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
                .when(self.visible_empty, |frame| {
                    let message = if self.query(cx).is_empty() {
                        self.model["empty"].as_str().unwrap_or_default().to_owned()
                    } else {
                        tr("lists.no_matches").to_owned()
                    };
                    frame.child(
                        div()
                            .id(SharedString::from(format!("{}-empty-overlay", self.id)))
                            .test_support()
                            .aria_label(message.clone())
                            .absolute()
                            .inset_0()
                            .p(px(SPACE_MD))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child(div().w_full().min_w_0().child(message)),
                    )
                }),
        );
        if let Some(buttons) = self.buttons(true, cx) {
            body = body.child(buttons);
        }
        body
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{key_spec, Node};
    use serde_json::{json, Value};

    fn node(kind: &str) -> Node {
        Node {
            id: format!("{kind}-1"),
            kind: kind.into(),
            label: kind.into(),
            detail: Value::Null,
            confirm: Value::Null,
        }
    }

    #[test]
    fn f2_picks_rename_group_rename_or_edit_for_node_kind() {
        let buttons = vec![
            json!({"id": "edit", "needs": ["action"]}),
            json!({"id": "group_rename", "needs": ["group"]}),
        ];
        assert_eq!(
            key_spec("f2", &buttons, &node("group")).map(|spec| spec["id"].clone()),
            Some(json!("group_rename"))
        );
        assert_eq!(
            key_spec("f2", &buttons, &node("action")).map(|spec| spec["id"].clone()),
            Some(json!("edit"))
        );
        assert!(key_spec("enter", &buttons, &node("group")).is_none());

        let with_rename = vec![
            json!({"id": "edit", "needs": ["action"]}),
            json!({"id": "rename", "needs": ["action"]}),
        ];
        assert_eq!(
            key_spec("f2", &with_rename, &node("action")).map(|spec| spec["id"].clone()),
            Some(json!("rename"))
        );
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn matching_ids(pane: &super::TreePane, cx: &gpui_kit::App) -> Vec<String> {
        fn collect(items: &[gpui_kit::component::tree::TreeItem], out: &mut Vec<String>) {
            for item in items {
                out.push(item.id.to_string());
                collect(&item.children, out);
            }
        }
        let query = pane.query(cx);
        let items = pane.items(&pane.model["nodes"], &query);
        let mut ids = Vec::new();
        collect(&items, &mut ids);
        ids
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn search_input(
        pane: &super::TreePane,
    ) -> Option<gpui_kit::Entity<gpui_kit::component::input::InputState>> {
        pane.search.clone()
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn is_visible_empty(pane: &super::TreePane) -> bool {
        pane.visible_empty
    }

    #[cfg(feature = "test-support")]
    mod ui {
        use super::super::{Options, TreePane};
        use super::{is_visible_empty, matching_ids, search_input};
        use gpui_kit::test::{TestQueryExt, TestWindowExt};
        use gpui_kit::{
            div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
            Window, WindowBounds, WindowOptions,
        };
        use serde_json::json;

        struct TreeHost(Entity<TreePane>);
        impl Render for TreeHost {
            fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
                div().size_full().p_4().child(self.0.clone())
            }
        }

        fn open_tree(
            model: serde_json::Value,
            searchable: bool,
            options: Options,
            cx: &mut TestAppContext,
        ) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<TreePane>) {
            cx.update(|cx| {
                gpui_kit::init(cx);
                crate::theme::apply(true, cx);
            });
            let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<TreePane>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let s = slot.clone();
            let window = cx.update(|cx| {
                let bounds = Bounds {
                    origin: gpui_kit::Point::default(),
                    size: size(px(480.), px(600.)),
                };
                let (window, _) = gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    cx,
                    move |window, cx| {
                        let pane = cx.new(|cx| TreePane::new("test", model, searchable, options, window, cx));
                        *s.borrow_mut() = Some(pane.clone());
                        cx.new(|_| TreeHost(pane))
                    },
                )
                .expect("open test window");
                window.downcast::<gpui_kit::base::Root>().expect("root window")
            });
            let pane = slot.borrow().clone().expect("pane created");
            (window, pane)
        }

        fn type_text(
            input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
            text: &str,
            window: &mut Window,
            cx: &mut gpui_kit::App,
        ) {
            use gpui_kit::test::TestEventExt;
            input.update(cx, |input, cx| input.focus(window, cx));
            window.key_down("ctrl-a", false, cx);
            window.key_up("ctrl-a", cx);
            if text.is_empty() {
                window.key_down("backspace", false, cx);
                window.key_up("backspace", cx);
            } else {
                window.input(text, cx);
            }
        }

        #[gpui_kit::test]
        fn search_filters_labels_details_and_shows_empty_overlay(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Pick Trigger",
                "searchHint": "Search triggers",
                "empty": "No triggers configured.",
                "nodes": [
                    {
                        "id": "cat-twitch",
                        "kind": "category",
                        "label": "Twitch",
                        "children": [
                            {
                                "id": "t-follow",
                                "kind": "item",
                                "label": "Follow",
                                "detail": {"text": "Fires when a new follower arrives."}
                            },
                            {
                                "id": "t-sub",
                                "kind": "item",
                                "label": "Subscription",
                                "detail": {"text": "Fires when someone subscribes."}
                            }
                        ]
                    },
                    {
                        "id": "cat-obs",
                        "kind": "category",
                        "label": "OBS Studio",
                        "children": [
                            {
                                "id": "t-scene",
                                "kind": "item",
                                "label": "Scene Changed",
                                "detail": {"text": "Active scene switched."}
                            }
                        ]
                    }
                ]
            });
            let (window, pane) = open_tree(
                model,
                true,
                Options {
                    closed: true,
                    ..Options::default()
                },
                cx,
            );
            let no_matches = crate::i18n::tr("lists.no_matches");
            let search = cx
                .update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(window.try_find_by_label(no_matches).is_none());
                    assert_eq!(
                        matching_ids(pane.read(cx), cx),
                        vec!["cat-twitch", "t-follow", "t-sub", "cat-obs", "t-scene"]
                    );
                    let search = search_input(pane.read(cx)).expect("search input");
                    // Search by item label filters tree and opens matching folder.
                    type_text(&search, "Scene", window, cx);
                    search
                })
                .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(matching_ids(pane.read(cx), cx), vec!["cat-obs", "t-scene"]);
                assert!(!is_visible_empty(pane.read(cx)));
                assert!(window.try_find_by_label(no_matches).is_none());

                // Search by detail text matches item.
                type_text(&search, "follower", window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(matching_ids(pane.read(cx), cx), vec!["cat-twitch", "t-follow"]);

                // Zero matching items must display the no-matches overlay even when nodes is non-empty.
                type_text(&search, "zzz-nomatch", window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(matching_ids(pane.read(cx), cx).is_empty());
                assert!(is_visible_empty(pane.read(cx)));
                let overlay = window
                    .try_find_by_label(no_matches)
                    .expect("no-matches overlay should render when search matches 0 items");
                assert_eq!(overlay.label(), Some(no_matches));

                // Clearing search restores all nodes and removes the overlay.
                type_text(&search, "", window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(!is_visible_empty(pane.read(cx)));
                assert!(window.try_find_by_label(no_matches).is_none());
                assert_eq!(
                    matching_ids(pane.read(cx), cx),
                    vec!["cat-twitch", "t-follow", "t-sub", "cat-obs", "t-scene"]
                );
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn external_filter_updates_non_searchable_tree(cx: &mut TestAppContext) {
            let model = json!({
                "title": "Triggers",
                "empty": "None",
                "nodes": [
                    {"id": "t1", "kind": "trigger", "label": "Chat Message"},
                    {"id": "t2", "kind": "trigger", "label": "Channel Follow"}
                ]
            });
            let (window, pane) = open_tree(model, false, Options::default(), cx);
            let no_matches = crate::i18n::tr("lists.no_matches");
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                pane.update(cx, |pane, cx| pane.set_filter("Follow", cx));
                window.render_frame(cx);
                assert_eq!(matching_ids(pane.read(cx), cx), vec!["t2"]);
                assert!(window.try_find_by_label(no_matches).is_none());

                pane.update(cx, |pane, cx| pane.set_filter("zzz", cx));
                window.render_frame(cx);
                assert!(matching_ids(pane.read(cx), cx).is_empty());
                assert!(window.try_find_by_label(no_matches).is_some());

                pane.update(cx, |pane, cx| pane.set_filter("", cx));
                window.render_frame(cx);
                assert_eq!(matching_ids(pane.read(cx), cx), vec!["t1", "t2"]);
                assert!(window.try_find_by_label(no_matches).is_none());
            })
            .unwrap();
        }
    }
}
