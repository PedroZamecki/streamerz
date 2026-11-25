//! One controller for every editable entity list, including selection, filtering and dialogs.
use super::{
    card::Card,
    form::Form,
    rows::{Action, Rows},
    sections::Sections,
};
use crate::{
    i18n::tr,
    send,
    theme::{
        CONTROL_HEIGHT, DENSE_CONTROL_HEIGHT, FONT_BODY, FONT_CAPTION, LINE_BODY, LINE_CAPTION, SPACE_MD, SPACE_SM,
        SPACE_XS,
    },
};
use gpui_kit::base::ElementExt;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    switch::Switch,
    table::{DataTable, TableEvent, TableState},
    ActiveTheme, Disableable, Icon, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};
use std::rc::Rc;

/// A change of the table's view (sort, hidden platforms) the page asks the core to apply to every row.
pub enum ViewChange {
    Sort {
        kind: String,
        column: String,
        desc: bool,
    },
    Hidden {
        kind: String,
        hidden: Vec<String>,
    },
    /// The page's "Show" combo picked another table.
    Tab(String),
    /// A row opened as a page of its own (an overlay): `key` in the view is its id.
    Open {
        key: String,
        id: String,
    },
}
impl EventEmitter<ViewChange> for List {}
impl EventEmitter<super::sections::Navigate> for List {}

pub struct List {
    model: Value,
    table: Option<Entity<TableState<Rows>>>,
    search: Option<Entity<InputState>>,
    /// The page's tabs (label, options, selected): drawn as a "Show" combo before the search.
    show: Option<Value>,
    lead_outside: bool,
    /// The title of the page this list is on, and whether it is the page's only section (the page's
    /// header then stands for it: no heading of its own, its page-wide buttons in the header).
    page_title: String,
    page_single: bool,
    /// The first list of the page: with no heading of its own, the page's header stands for it.
    page_lead: bool,
    selected_id: String,
    form: Option<Entity<Form>>,
    error: Option<crate::error::StreamerzError>,
    route: String,
    /// A list in a dialog (a group's members): its entity and the request that fills it.
    dialog_list: Option<Entity<List>>,
    dialog_list_sub: Option<Subscription>,
    dialog_request: Value,
    /// A person's card open in a dialog.
    card: Option<Entity<Card>>,
    /// Fields drawn in place of a settings row (the MCP connection's address, port and route).
    inline: Option<Entity<Sections>>,
    rows_view: Entity<ButtonRows>,
    actions_view: Entity<HeaderActions>,
    _subscriptions: Vec<Subscription>,
}
/// The height of a table's header row, and how many rows a table sized to its rows shows at least and at most.
const TABLE_HEAD: f32 = 24.;
const FEWEST_ROWS: usize = 3;
const MOST_ROWS: usize = 8;
/// The height of a table sized to its `shown` rows: at least a few rows, `most` at most (it scrolls then).
fn table_height(shown: usize, most: usize) -> f32 {
    TABLE_HEAD + CONTROL_HEIGHT * shown.clamp(FEWEST_ROWS, most.max(FEWEST_ROWS)) as f32 + 1.
}
/// About how wide a button with `label` is (icon, padding and text at the small size).
fn label_width(label: &str) -> f32 {
    52. + 7. * label.chars().count() as f32
}
/// A small rounded tag of a row (a lock, a retry time, the default star): an optional icon and a short text.
fn pill(id: impl Into<ElementId>, icon: Option<&str>, text: Option<String>, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(SPACE_XS))
        .flex_shrink_0()
        .h(px(crate::theme::LINE_MICRO))
        .px(px(SPACE_XS))
        .rounded(px(4.))
        .bg(crate::theme::badge_bg(cx))
        .text_color(crate::theme::quiet(cx))
        .when_some(icon, |tag, icon| {
            tag.child(
                Icon::empty()
                    .path(format!("icons/{icon}.svg"))
                    .size(px(crate::theme::FONT_MICRO)),
            )
        })
        .when_some(text, |tag, text| {
            tag.child(
                div()
                    .text_size(px(crate::theme::FONT_MICRO))
                    .line_height(px(crate::theme::LINE_MICRO))
                    .child(text),
            )
        })
}

/// The blocks of a page's list from top to bottom. The gap under each block is the one docs/UI.md
/// ("Anatomy") names for it, and none is drawn for a block that is not there.
struct Stack {
    items: Vec<AnyElement>,
    after: f32,
}
impl Stack {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            after: 0.,
        }
    }
    /// Adds a block, with `after` px between it and the next one.
    fn add(&mut self, block: impl IntoElement, after: f32) {
        if !self.items.is_empty() && self.after > 0. {
            self.items
                .push(div().h(px(self.after)).flex_shrink_0().into_any_element());
        }
        self.items.push(block.into_any_element());
        self.after = after;
    }
    /// What was added so far, as a column of its own.
    fn column(&mut self) -> Div {
        let items = std::mem::take(&mut self.items);
        self.after = 0.;
        div().flex().flex_col().children(items)
    }
}
impl List {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.entity().downgrade();
        let action = Rc::new(move |action, row, window: &mut Window, cx: &mut App| {
            let _ = weak.update(cx, |list, cx| list.action(action, Some(row), window, cx));
        });
        let sorter = {
            let weak = cx.entity().downgrade();
            Rc::new(move |column: String, desc: bool, _: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |list, cx| {
                    let kind = list.kind().to_owned();
                    cx.emit(ViewChange::Sort { kind, column, desc });
                });
            })
        };
        let table = (model["presentation"] != "settings"
            && model["presentation"] != "connections"
            && model["presentation"] != "cards")
            .then(|| {
                cx.new(|cx| {
                    TableState::new(Rows::new(model.clone(), action, sorter), window, cx)
                        .row_selectable(true)
                        .col_selectable(false)
                        .sortable(false)
                })
            });
        let search = (model["search"] == true).then(|| {
            let hint = model["searchHint"]
                .as_str()
                .map_or_else(|| tr("lists.search").to_owned(), str::to_owned);
            cx.new(|cx| InputState::new(window, cx).placeholder(hint))
        });
        let mut subscriptions = Vec::new();
        if let Some(table) = &table {
            subscriptions.push(
                cx.subscribe_in(table, window, |list, table, event, window, cx| match event {
                    TableEvent::SelectRow(row) => {
                        list.selected_id = table.read(cx).delegate().id(*row).unwrap_or_default().to_owned();
                        cx.notify();
                    }
                    // A right click also picks the row for the buttons, as the production lists do.
                    TableEvent::RightClickedRow(Some(row)) => {
                        list.selected_id = table.read(cx).delegate().id(*row).unwrap_or_default().to_owned();
                        cx.notify();
                    }
                    TableEvent::DoubleClickedRow(row) => {
                        // A model can name the action a double click runs (the person's card); else edit.
                        let open = list.model["open"].as_str().and_then(|id| {
                            list.model["actions"]
                                .as_array()?
                                .iter()
                                .position(|action| action["id"] == id)
                        });
                        list.action(open.map_or(Action::Edit, Action::Custom), Some(*row), window, cx)
                    }
                    _ => {}
                }),
            );
        }
        if let Some(search) = &search {
            subscriptions.push(cx.subscribe(search, |list, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    list.refresh(cx);
                }
            }));
        }
        let inline = model["inline"].is_object().then(|| {
            let (inline, route) = (model["inline"].clone(), format!("{route}:inline"));
            cx.new(|cx| Sections::new(inline, route, window, cx))
        });
        let rows_view = {
            let weak = cx.entity().downgrade();
            cx.new(|_| ButtonRows(weak))
        };
        let actions_view = {
            let weak = cx.entity().downgrade();
            cx.new(|_| HeaderActions(weak))
        };
        let mut this = Self {
            model,
            table,
            search,
            show: None,
            lead_outside: false,
            page_title: String::new(),
            page_single: false,
            page_lead: false,
            selected_id: String::new(),
            form: None,
            error: None,
            route,
            dialog_list: None,
            dialog_list_sub: None,
            dialog_request: Value::Null,
            card: None,
            inline,
            rows_view,
            actions_view,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        this
    }
    /// Computes the next `hidden` (or active-toggle) list when option `index` of `filter` is clicked.
    /// In default mode (platforms), `hidden` lists hidden items and at least one stays shown.
    /// In `"mode": "toggles"` mode, `hidden` lists active toggles and any subset (including none) is valid.
    fn next_filter_hidden(filter: &Value, index: usize) -> Option<Vec<String>> {
        let options = filter["options"].as_array()?;
        let id = options.get(index).and_then(|option| option["id"].as_str())?;
        if filter["mode"] == "toggles" {
            let mut active: Vec<String> = options
                .iter()
                .filter(|option| option["shown"] == true)
                .filter_map(|option| option["id"].as_str().map(str::to_owned))
                .collect();
            if let Some(position) = active.iter().position(|item| item == id) {
                active.remove(position);
            } else {
                active.push(id.to_owned());
            }
            return Some(active);
        }
        let mut hidden: Vec<String> = options
            .iter()
            .filter(|option| option["shown"] == false)
            .filter_map(|option| option["id"].as_str().map(str::to_owned))
            .collect();
        if let Some(position) = hidden.iter().position(|hidden| hidden == id) {
            hidden.remove(position);
        } else if hidden.len() + 1 < options.len() {
            hidden.push(id.to_owned());
        } else {
            return None;
        }
        Some(hidden)
    }

    /// Shows or hides the `index`th option of the model's filter (or toggles it in toggle mode).
    fn toggle_filter(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(hidden) = Self::next_filter_hidden(&self.model["filter"], index) else {
            return;
        };
        let kind = self.kind().to_owned();
        cx.emit(ViewChange::Hidden { kind, hidden });
    }

    /// Clamps a progress model's percentage into `0.0..=100.0`, returning `None` if absent/invalid.
    fn progress_percent(progress: &Value) -> Option<f32> {
        let pct = progress["percent"].as_f64()?;
        pct.is_finite().then_some(pct.clamp(0.0, 100.0) as f32)
    }

    /// Shared live progress bar for lists with background work (e.g. voice downloads).
    fn progress_bar(progress: &Value, cx: &App) -> Option<AnyElement> {
        let percent = Self::progress_percent(progress)?;
        let label = progress["label"].as_str().unwrap_or_default().to_owned();
        let pct_text = format!("{}%", percent.round() as u32);
        Some(
            div()
                .id("list-progress")
                .test_support()
                .role(Role::Group)
                .aria_label(label.clone())
                .w_full()
                .flex()
                .flex_col()
                .gap(px(SPACE_XS))
                .px(px(SPACE_MD))
                .py(px(SPACE_SM))
                .rounded(px(6.))
                .bg(crate::theme::row_surface(cx))
                .border_1()
                .border_color(crate::theme::row_border(cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(SPACE_SM))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_SM))
                                .min_w_0()
                                .flex_1()
                                .child(
                                    Icon::empty()
                                        .path("icons/download.svg")
                                        .size(px(13.))
                                        .text_color(cx.theme().primary),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(FONT_CAPTION))
                                        .line_height(px(LINE_CAPTION))
                                        .text_color(cx.theme().foreground)
                                        .child(label),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(FONT_CAPTION))
                                .line_height(px(LINE_CAPTION))
                                .text_color(crate::theme::quiet(cx))
                                .child(pct_text),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(6.))
                        .rounded(px(3.))
                        .bg(cx.theme().muted)
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .w(relative(percent / 100.))
                                .rounded(px(3.))
                                .bg(cx.theme().primary),
                        ),
                )
                .into_any_element(),
        )
    }
    pub fn is_settings(&self) -> bool {
        self.model["presentation"] == "settings"
    }
    pub fn is_connections(&self) -> bool {
        self.model["presentation"] == "connections"
    }
    pub fn is_cards(&self) -> bool {
        self.model["presentation"] == "cards"
    }
    pub fn kind(&self) -> &str {
        self.model["kind"].as_str().unwrap_or_default()
    }
    /// A list that carries a connection state and buttons under its table (Stream Apps).
    pub fn has_connection(&self) -> bool {
        self.model["connection"].is_object()
    }
    /// Whether the list has buttons under it (a read-only list, like pending items, has none).
    pub fn buttons_last(&self) -> bool {
        self.model["buttonsLast"] == true
    }
    /// Leave room for a section's own introduction and status as well as its empty-state actions.
    /// Multi-section pages scroll when these minimums do not fit.
    pub fn section_min_height(&self) -> f32 {
        if !self.lead_outside && self.model["intro"].is_string() && self.model["status"].is_object() {
            240.
        } else if !self.lead_outside && (self.model["intro"].is_string() || self.model["status"].is_object()) {
            200.
        } else {
            140.
        }
    }
    /// Whether the list is as tall as its table needs, not as the page lets it: a table that shares its
    /// page with other sections. The page then lets the section take no more room than that.
    pub fn hugs(&self) -> bool {
        !self.page_single
            && self.table.is_some()
            && !self.is_settings()
            && !self.is_connections()
            && !self.is_cards()
            && !self.has_inline()
            && !self.has_connection()
    }
    pub fn has_inline(&self) -> bool {
        self.model["inline"].is_object()
    }
    /// A page's introduction and its state line (above the first table).
    pub fn intro_block(&self, cx: &App) -> Option<AnyElement> {
        self.intro_with(false, cx)
    }
    /// The introduction as the page's guide (13/18), or, under a section's own heading, as its note (12/16).
    fn intro_with(&self, note: bool, cx: &App) -> Option<AnyElement> {
        if self.model["intro"].is_null() && !self.model["status"].is_object() {
            return None;
        }
        let mut block = div().flex().flex_col().gap(px(SPACE_SM));
        if let Some(intro) = self.model["intro"].as_str() {
            block = block.child(if note {
                super::notice::note(intro, cx)
            } else {
                super::page::guide(intro, cx)
            });
        }
        if self.model["status"].is_object() {
            let (icon, color) =
                super::health::appearance(self.model["status"]["health"].as_str().unwrap_or_default(), cx);
            block = block.child(
                div()
                    .id("table-status")
                    .test_support()
                    .role(Role::Group)
                    .aria_label(self.model["status"]["text"].as_str().unwrap_or_default().to_owned())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_SM))
                    .text_size(px(FONT_BODY))
                    .line_height(px(LINE_BODY))
                    .text_color(color)
                    .child(
                        gpui_kit::component::Icon::empty()
                            .path(format!("icons/{icon}.svg"))
                            .small(),
                    )
                    .child(self.model["status"]["text"].as_str().unwrap_or_default().to_owned()),
            );
        }
        Some(block.into_any_element())
    }
    /// The page draws the introduction itself (above sections that share the height evenly).
    pub fn draw_intro_outside(&mut self) {
        self.lead_outside = true;
    }
    /// The page this list is on: its title, and whether the list is its only section. The page sets it
    /// on every frame, before the list draws.
    pub fn set_page(&mut self, title: String, single: bool, lead: bool) {
        if self.page_title != title {
            self.page_title = title;
        }
        self.page_single = single;
        self.page_lead = lead;
        self.lead_outside = false;
    }
    /// Whether the list's title is the page's own (the page's header stands for it).
    fn is_page_title(&self) -> bool {
        let title = self.model["title"].as_str().unwrap_or_default();
        !title.is_empty() && title.to_lowercase() == self.page_title.to_lowercase()
    }
    /// Whether the list draws its own heading (H2). Only a page with more than one section has headings,
    /// never one that repeats the page's title, except a connection's, which carries its state.
    fn heading_visible(&self) -> bool {
        let guide = self.model["guide"].as_str().unwrap_or_default();
        let title = self.model["title"].as_str().unwrap_or_default();
        let wanted = !title.is_empty()
            && ((!guide.is_empty() && self.model["titleless"] != true) || self.model["showTitle"] == true);
        let same = self.is_page_title();
        let carries_state = self.model["headerAdd"] == true && self.has_connection();
        wanted && !same && (!self.page_single || carries_state)
    }
    /// The page-wide buttons: Add when the model asks for it in the header, else the top buttons, when
    /// nothing else (search, filter, tabs) shares their line.
    fn header_add(&self) -> bool {
        self.model["headerAdd"] == true
    }
    fn top_in_header(&self, cx: &App) -> bool {
        !self.header_add()
            && self.search.is_none()
            && self.model["filter"]["options"].as_array().is_none_or(Vec::is_empty)
            && self.show.is_none()
            && !self.empty_state(cx)
            && !self.top_actions().is_empty()
    }
    /// Whether the page's header carries this list's page-wide buttons: it is the page's only section
    /// and has no heading of its own to hold them.
    pub fn hosts_actions(&self, cx: &App) -> bool {
        (self.page_single || self.page_lead || self.is_page_title())
            && !self.heading_visible()
            && !self.empty_state(cx)
            && (self.header_add() || self.top_in_header(cx))
    }
    /// The page-wide buttons as their own view, for the page's header.
    pub fn header_actions_view(&self) -> Entity<HeaderActions> {
        self.actions_view.clone()
    }
    pub fn set_show(&mut self, tabs: Option<Value>, cx: &mut Context<Self>) {
        if self.show != tabs {
            self.show = tabs;
            cx.notify();
        }
    }
    /// Sizes the table's columns to `width` before layout so the first frame and resizes match it.
    pub fn set_fit(&mut self, width: f32, cx: &mut Context<Self>) {
        let Some(table) = &self.table else {
            return;
        };
        let target = if self.model["detailSide"] == true && width >= 600. {
            260.
        } else {
            width
        };
        if target > 1. && (target - table.read(cx).delegate().fit).abs() > 1. {
            table.update(cx, |table, cx| {
                table.delegate_mut().fit = target;
                table.delegate_mut().layout();
                table.refresh(cx);
                cx.notify();
            });
            cx.notify();
        }
    }
    pub fn update_model(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        if model["inline"].is_object() {
            match &self.inline {
                Some(inline) => inline.update(cx, |inline, cx| {
                    inline.update_model(model["inline"].clone(), window, cx)
                }),
                None => {
                    let route = format!("{}:inline", self.route);
                    let inline = model["inline"].clone();
                    self.inline = Some(cx.new(|cx| Sections::new(inline, route, window, cx)));
                }
            }
        } else {
            self.inline = None;
        }
        self.model = model;
        self.refresh(cx);
    }
    /// Whether the list shows its empty state (what to do first) instead of a table.
    fn empty_state(&self, cx: &App) -> bool {
        !self.is_connections() && !self.is_cards() && self.truly_empty(cx)
    }
    fn truly_empty(&self, cx: &App) -> bool {
        self.model["rows"].as_array().is_some_and(Vec::is_empty)
            && self
                .search
                .as_ref()
                .is_none_or(|search| search.read(cx).value().trim().is_empty())
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self
            .search
            .as_ref()
            .map(|search| search.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let mut model = self.model.clone();
        if !query.is_empty() {
            if let Some(rows) = model["rows"].as_array_mut() {
                rows.retain(|row| {
                    row["cells"].as_array().into_iter().flatten().any(|cell| {
                        cell.as_str()
                            .or_else(|| cell["text"].as_str())
                            .is_some_and(|text| text.to_lowercase().contains(&query))
                            || cell["platforms"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .any(|platform| platform.to_lowercase().contains(&query))
                            || cell["brand"]
                                .as_str()
                                .is_some_and(|brand| brand.to_lowercase().contains(&query))
                    })
                });
            }
            model["empty"] = tr("lists.no_matches").into();
        }
        let mut selection = model["rows"].as_array().and_then(|rows| {
            rows.iter()
                .position(|row| !self.selected_id.is_empty() && row["id"] == self.selected_id)
        });
        // A page that always has one thing open (the MCP agents) picks the first row.
        if selection.is_none() && model["selectFirst"] == true {
            if let Some(id) = model["rows"][0]["id"].as_str() {
                self.selected_id = id.to_owned();
                selection = Some(0);
            }
        }
        if selection.is_none() {
            self.selected_id.clear();
        }
        if let Some(table) = &self.table {
            table.update(cx, |table, cx| {
                table.delegate_mut().model = model;
                table.delegate_mut().layout();
                table.refresh(cx);
                cx.notify();
                if let Some(row) = selection {
                    if table.selected_row() != Some(row) {
                        table.set_selected_row(row, cx);
                    }
                } else if table.selected_row().is_some() {
                    table.clear_selection(cx);
                }
            });
        }
        cx.notify();
    }
    /// Resolves the explicit row (from a context menu or double-click) or the currently selected row
    /// (from a toolbar/bottom button) so per-row `confirm`, `kind` and `singleton` metadata apply to both.
    /// An explicit out-of-bounds row index returns `None` instead of acting on a different selected row.
    fn row_record(shown: &Value, row: Option<usize>, selected_id: &str) -> Option<Value> {
        match row {
            Some(row) => shown["rows"].get(row).cloned(),
            None => shown["rows"]
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| !selected_id.is_empty() && row["id"] == selected_id)
                })
                .cloned(),
        }
    }
    /// Whether `action` is valid for `shown` (checking exact index for custom actions).
    fn action_allowed(shown: &Value, action: Action) -> bool {
        Action::base_actions(shown).iter().any(|listed| match (listed, action) {
            (Action::Custom(a), Action::Custom(b)) => *a == b,
            _ => std::mem::discriminant(listed) == std::mem::discriminant(&action),
        }) || (matches!(action, Action::Toggle) && shown["switchToggle"] == true)
            || matches!(action, Action::Connect | Action::ConnectAll | Action::DisconnectAll)
    }
    /// Confirmation question and button label for a custom action, preferring per-row prompts.
    fn confirm_prompt(spec: &Value, record: Option<&Value>) -> Option<(String, String)> {
        let action_id = spec["id"].as_str().unwrap_or_default();
        let own = record.and_then(|row| row["confirm"][action_id].as_str());
        let question = own.or_else(|| spec["confirm"].as_str())?;
        let label = spec["confirmLabel"]
            .as_str()
            .or_else(|| spec["label"].as_str())
            .unwrap_or_default();
        Some((question.to_owned(), label.to_owned()))
    }
    fn action(&mut self, action: Action, row: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let shown = match &self.table {
            Some(table) => table.read(cx).delegate().model.clone(),
            None => self.model.clone(),
        };
        let record = Self::row_record(&shown, row, &self.selected_id);
        let kind = record
            .as_ref()
            .and_then(|row| row["kind"].as_str())
            .unwrap_or_else(|| self.kind())
            .to_owned();
        let id = record
            .as_ref()
            .and_then(|row| row["id"].as_str())
            .unwrap_or(if row.is_some() { "" } else { &self.selected_id })
            .to_owned();
        let selected = shown["rows"]
            .as_array()
            .and_then(|rows| rows.iter().position(|row| !id.is_empty() && row["id"] == id));
        if !Self::action_allowed(&shown, action) {
            return;
        }
        if !action.enabled(&shown, selected) {
            return;
        }
        let route = self.route.clone();
        match action {
            Action::Add | Action::Edit => send(
                json!({"request": "StreamerzEntityForm", "kind": kind, "itemId": if matches!(action, Action::Add) || record.as_ref().is_some_and(|row| row["singleton"] == true) { "" } else { &id }, "id": format!("{route}:form")}),
            ),
            Action::Toggle => send(
                json!({"request": "StreamerzToggleEntity", "kind": kind, "itemId": id, "id": format!("{route}:toggle")}),
            ),
            Action::Refresh if !self.model["connection"].is_object() => send(
                json!({"request": "StreamerzCoreAction", "domain": self.model["domain"], "action": "refresh", "id": format!("{route}:act")}),
            ),
            Action::Custom(index) => {
                let spec = self.model["actions"][index].clone();
                if let Some(key) = spec["view"].as_str() {
                    cx.emit(ViewChange::Open {
                        key: key.to_owned(),
                        id: id.clone(),
                    });
                    return;
                }
                if let Some(page) = spec["navigate"].as_str() {
                    cx.emit(super::sections::Navigate(page.to_owned()));
                    return;
                }
                if let Some(domain) = spec["dialog"].as_str() {
                    self.dialog_request = json!({"request": "StreamerzCoreView", "domain": domain, "itemId": id});
                    let mut request = self.dialog_request.clone();
                    request["id"] = format!("{route}:dialog-view").into();
                    send(request);
                    return;
                }
                // A form of exactly this kind, whatever row is selected.
                if let Some(entity) = spec["entity"].as_str() {
                    send(
                        json!({"request": "StreamerzEntityForm", "kind": entity, "itemId": "", "id": format!("{route}:form")}),
                    );
                    return;
                }
                if let Some(prefix) = spec["form"].as_str() {
                    send(
                        json!({"request": "StreamerzEntityForm", "kind": format!("{prefix}{id}"), "itemId": "", "id": format!("{route}:form")}),
                    );
                    return;
                }
                let request = json!({"request": "StreamerzCoreAction", "domain": self.model["domain"], "table": kind,
                    "action": spec["id"], "itemId": id, "id": format!("{route}:act")});
                // A row may ask its own question (the last page of an overlay, one with actions).
                match Self::confirm_prompt(&spec, record.as_ref()) {
                    None => send(request),
                    Some((question, label)) => {
                        let danger = spec["danger"] == true;
                        window.open_dialog(cx, move |dialog, window, cx| {
                            let request = request.clone();
                            let confirm =
                                Button::new("confirm-action")
                                    .label(label.clone())
                                    .on_click(move |_, window, cx| {
                                        send(request.clone());
                                        window.close_dialog(cx);
                                    });
                            let buttons = div()
                                .id("action-confirmation")
                                .test_support()
                                .role(Role::Group)
                                .aria_label(label.clone())
                                .flex()
                                .justify_end()
                                .gap(px(SPACE_SM))
                                .child(
                                    Button::new("cancel-action")
                                        .label(tr("dialogs.cancel"))
                                        .on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx)),
                                )
                                .child(if danger { confirm.danger() } else { confirm });
                            let footer = crate::controls::dialog::footer(None, buttons, cx);
                            crate::controls::dialog::shell(dialog, label.clone(), 448., window, cx)
                                .child(crate::controls::dialog::body(question.clone()))
                                .footer(footer)
                        });
                    }
                }
            }
            Action::Connect | Action::ConnectAll | Action::DisconnectAll | Action::Refresh => {
                let operation = match action {
                    Action::Refresh => "refresh",
                    Action::DisconnectAll => "disconnect",
                    Action::Connect => match selected {
                        Some(row) if shown["rows"][row]["connected"] == true => "disconnect",
                        Some(_) => "connect",
                        None if shown["connection"]["canConnect"] == true => "connect",
                        None => "disconnect",
                    },
                    _ => "connect",
                };
                let target_id = match (action, selected) {
                    (Action::Connect, Some(_)) => id.as_str(),
                    _ => "",
                };
                send(
                    json!({"request": "StreamerzConnection", "kind": kind, "itemId": target_id, "action": operation, "id": format!("{route}:connection")}),
                );
            }
            Action::Delete => {
                let question = tr("lists.delete_confirm").replace(
                    "{what}",
                    &self.model["what"].as_str().unwrap_or_default().to_lowercase(),
                );
                let name = self.model["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|row| row["id"] == id)
                    .and_then(|row| row["name"].as_str())
                    .unwrap_or_default();
                let confirm_label = tr("btn.delete_named").replace("{name}", name);
                window.open_dialog(cx, move |dialog, window, cx| {
                    let kind = kind.clone(); let id = id.clone(); let route = route.clone();
                    let buttons = div().id("delete-confirmation").test_support().role(Role::Group).aria_label(confirm_label.clone()).flex().justify_end().gap(px(SPACE_SM))
                        .child(Button::new("cancel-delete").label(tr("dialogs.cancel")).on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx)))
                        .child(Button::new("confirm-delete").danger().label(tr("btn.delete")).accessibility_label(confirm_label.clone()).on_click(move |_, window, cx| { send(json!({"request": "StreamerzDelete", "what": kind, "itemId": id, "id": format!("{route}:delete")})); window.close_dialog(cx); }));
                    let footer = crate::controls::dialog::footer(None, buttons, cx);
                    crate::controls::dialog::shell(dialog, tr("btn.delete"), 448., window, cx)
                        .child(crate::controls::dialog::body(question.clone()))
                        .footer(footer)
                });
            }
        }
    }
    /// Returns whether the page should query the core again.
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some((index, what)) = operation.strip_prefix("gallery:").and_then(|rest| rest.split_once(':')) {
            if let (Ok(index), Some(form)) = (index.parse::<usize>(), &self.form) {
                form.update(cx, |form, cx| form.gallery_reply(index, what, &value, cx));
            }
            return false;
        }
        if let Some(index) = operation.strip_prefix("press:").and_then(|s| s.parse::<usize>().ok()) {
            if let Some(form) = &self.form {
                form.update(cx, |form, cx| form.press_reply(index, &value, window, cx));
            }
            return false;
        }
        if let Some(err) = crate::error::StreamerzError::from_response(&value) {
            if matches!(operation, "save" | "json") {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.pending = false;
                        form.error = Some(err);
                        cx.notify();
                    });
                }
            } else {
                self.error = Some(err);
            }
            cx.notify();
            return false;
        }
        self.error = None;
        if let (Some(rest), Some(inline)) = (operation.strip_prefix("inline:"), self.inline.clone()) {
            return inline.update(cx, |inline, cx| inline.apply(rest, value, window, cx));
        }
        if let Some(rest) = operation.strip_prefix("dlist:") {
            let Some(nested) = self.dialog_list.clone() else {
                return false;
            };
            if nested.update(cx, |list, cx| list.apply(rest, value, window, cx)) {
                let mut request = self.dialog_request.clone();
                request["id"] = format!("{}:dialog-refresh", self.route).into();
                send(request);
                return true;
            }
            return false;
        }
        match operation {
            "dialog-view" if value["card"].is_object() => {
                let card = cx.new(|_| Card::new(value["card"].clone(), self.route.clone()));
                self.card = Some(card.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, window, _| {
                    let owner_cancel = owner.clone();
                    let owner_close = owner.clone();
                    dialog
                        .centered()
                        .close_button(false)
                        .pt(px(0.))
                        .pb(px(0.))
                        .pl(px(0.))
                        .pr(px(0.))
                        .width(px(crate::controls::dialog::fit_width(520., window)))
                        .child(card.clone())
                        .on_cancel(move |_, _, cx| {
                            let _ = owner_cancel.update(cx, |list, _| list.card = None);
                            true
                        })
                        .on_close(move |_, _, cx| {
                            let _ = owner_close.update(cx, |list, _| list.card = None);
                        })
                });
                false
            }
            "card-lookup" => {
                if let Some(card) = self.card.clone() {
                    card.update(cx, |card, cx| card.lookup(value, cx));
                }
                false
            }
            "card-images" => {
                if let Some(card) = self.card.clone() {
                    card.update(cx, |card, cx| card.images(&value, cx));
                }
                false
            }
            "card-act" => {
                // A group changed: draw the card again with the core's answer.
                let mut request = self.dialog_request.clone();
                request["id"] = format!("{}:dialog-refresh", self.route).into();
                send(request);
                false
            }
            "dialog-view" => {
                let Some(model) = value["tables"].get(0).filter(|table| table.is_object()).cloned() else {
                    return false;
                };
                let title = model["title"].as_str().unwrap_or_default().to_owned();
                let width = model["dialogWidth"].as_f64().unwrap_or(560.) as f32;
                let nested = cx.new(|cx| {
                    let mut list = List::new(model, format!("{}:dlist", self.route), window, cx);
                    let fit = crate::controls::dialog::fit_width(width, window) - 2. * crate::theme::SPACE_LG - 2.;
                    list.set_fit(fit, cx);
                    list
                });
                self.dialog_list_sub = Some(cx.subscribe_in(
                    &nested,
                    window,
                    |_, _, event: &super::sections::Navigate, window, cx| {
                        super::dialog::dismiss(window, cx);
                        cx.emit(super::sections::Navigate(event.0.clone()));
                    },
                ));
                self.dialog_list = Some(nested.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, window, cx| {
                    let owner_cancel = owner.clone();
                    let owner_close = owner.clone();
                    let max_body = crate::controls::dialog::fit_height(180., 180., 680., window);
                    crate::controls::dialog::shell(dialog, title.clone(), width, window, cx)
                        .child(crate::controls::dialog::body(
                            div()
                                .id("dialog-list")
                                .w_full()
                                .max_h(px(max_body))
                                .overflow_y_scroll()
                                .child(nested.clone()),
                        ))
                        .footer(crate::controls::dialog::close_footer(cx))
                        .on_cancel(move |_, _, cx| {
                            let _ = owner_cancel.update(cx, |list, _| {
                                list.dialog_list = None;
                                list.dialog_list_sub = None;
                            });
                            true
                        })
                        .on_close(move |_, _, cx| {
                            let _ = owner_close.update(cx, |list, _| {
                                list.dialog_list = None;
                                list.dialog_list_sub = None;
                            });
                        })
                });
                false
            }
            "dialog-refresh" if value["card"].is_object() => {
                if let Some(card) = &self.card {
                    card.update(cx, |card, cx| card.update_model(value["card"].clone(), cx));
                }
                false
            }
            "dialog-refresh" => {
                if let (Some(nested), Some(model)) = (
                    &self.dialog_list,
                    value["tables"].get(0).filter(|table| table.is_object()).cloned(),
                ) {
                    nested.update(cx, |list, cx| list.update_model(model, window, cx));
                }
                false
            }
            "form" => {
                let title = value["title"].as_str().unwrap_or_default().to_owned();
                let route = format!("{}:save", self.route);
                let form = cx.new(|cx| Form::new(value, route, window, cx));
                self.form = Some(form.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, window, cx| {
                    let owner_close = owner.clone();
                    crate::controls::dialog::shell(dialog, title.clone(), 560., window, cx)
                        .child(crate::controls::dialog::body(form.clone()))
                        .footer(form.read(cx).buttons_view())
                        .on_cancel({
                            let form = form.clone();
                            let owner = owner.clone();
                            move |_, _, cx| {
                                if form.read(cx).pending {
                                    return false;
                                }
                                let _ = owner.update(cx, |list, cx| {
                                    list.form = None;
                                    cx.notify();
                                });
                                true
                            }
                        })
                        .on_close(move |_, _, cx| {
                            let _ = owner_close.update(cx, |list, _| list.form = None);
                        })
                });
                false
            }
            "json" => {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.show_json(value["json"].as_str().unwrap_or_default(), window, cx)
                    });
                }
                false
            }
            "save" => {
                if let Some(id) = value["saved"].as_str() {
                    self.selected_id = id.to_owned();
                }
                self.form = None;
                window.close_dialog(cx);
                true
            }
            "act" => {
                if value["values"].is_object() {
                    show_values(&value["values"], window, cx);
                }
                if let Some(text) = value["copy"].as_str() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                    if let Some(message) = value["message"].as_str() {
                        window.push_notification(message.to_owned(), cx);
                    }
                }
                true
            }
            "toggle" | "delete" | "connection" => true,
            _ => false,
        }
    }
}
impl List {
    /// Width available for this list's table, toolbar and button rows.
    fn available_width(&self, window: &Window, cx: &App) -> f32 {
        let fit = self.table.as_ref().map_or(0., |table| table.read(cx).delegate().fit);
        if fit > 0. {
            fit
        } else {
            f32::from(window.viewport_size().width) - 200.
        }
    }
    /// Whether the core put this button on the line above the list (what starts or brings in
    /// something: New, Import), not among those for the selected row.
    fn on_top(&self, action: Action) -> bool {
        matches!(action, Action::Custom(index) if self.model["actions"][index]["top"] == true)
            || (matches!(action, Action::Add) && (self.model["headerAdd"] == true || self.model["addTop"] == true))
    }
    fn hide_disabled(&self, action: Action) -> bool {
        matches!(action, Action::Custom(index) if self.model["actions"][index]["hideDisabled"] == true)
    }
    /// The buttons of the line above the list, at the right of the search. They show their words
    /// (one marked `iconOnly` never does) and fold to icons together when the line is short.
    fn top_actions(&self) -> Vec<Action> {
        let header_add = self.model["headerAdd"] == true;
        Action::base_actions(&self.model)
            .into_iter()
            .filter(|action| {
                self.on_top(*action)
                    && !(header_add && matches!(action, Action::Add))
                    && (!self.hide_disabled(*action) || action.enabled(&self.model, None))
            })
            .collect()
    }
    fn top_buttons(&self, reserved: f32, window: &mut Window, cx: &mut Context<Self>) -> Vec<Button> {
        let actions = self.top_actions();
        let icon_only = |action: Action| matches!(action, Action::Custom(index) if self.model["actions"][index]["iconOnly"] == true);
        let wanted: f32 = actions
            .iter()
            .map(|action| {
                if icon_only(*action) {
                    40.
                } else {
                    label_width(&action.label(&self.model))
                }
            })
            .sum();
        let room = self.available_width(window, cx) - reserved;
        // The search keeps room to type in.
        let fold = wanted + 8. * actions.len() as f32 + 180. > room;
        actions
            .into_iter()
            .map(|action| {
                let label = action.label(&self.model);
                let words = !fold && !icon_only(action);
                let primary = matches!(action, Action::Custom(index) if self.model["actions"][index]["primary"] == true)
                    || (matches!(action, Action::Add)
                        && self.model["addTop"] == true
                        && self.model["addPrimary"] != false);
                let tip = action.tip(&self.model);
                Button::new(SharedString::from(format!("top-{}", action.id(&self.model))))
                    .small()
                    .h(px(CONTROL_HEIGHT))
                    .flex_shrink_0()
                    .when(words, |button| button.label(label.clone()))
                    .when(!words, |button| {
                        button.w(px(34.)).tooltip(tip.clone().unwrap_or_else(|| label.clone()))
                    })
                    .when(words, |button| button.when_some(tip, |button, tip| button.tooltip(tip)))
                    .when(primary, |button| button.primary())
                    .accessibility_label(label)
                    .icon(action.icon(&self.model))
                    .disabled(!action.enabled(&self.model, None))
                    .on_click(cx.listener(move |list, _, window, cx| list.action(action, None, window, cx)))
            })
            .collect()
    }
    /// The buttons at the right of a heading or of the page's header: Add (with Connect all on a
    /// connection list) when the model puts it there, else the top buttons given.
    fn heading_actions(&self, header_add: bool, top: Vec<Button>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let gap = px(crate::theme::SPACE_SM);
        let size = px(crate::theme::CONTROL_HEIGHT);
        if header_add {
            let add_label = Action::Add.label(&self.model);
            if self.is_connections() || self.is_cards() {
                let mut actions = div().flex().items_center().gap(gap).flex_shrink_0();
                if self.is_connections() && self.model["rows"].as_array().map_or(0, Vec::len) > 1 {
                    let can_connect = self.model["connection"]["canConnect"] == true;
                    let conn_icon = if can_connect { "plug" } else { "unplug" };
                    let conn_label = tr(if can_connect {
                        "btn.connect_all"
                    } else {
                        "btn.disconnect_all"
                    });
                    let conn_color = if can_connect {
                        crate::theme::quiet(cx)
                    } else {
                        super::health::appearance("up", cx).1
                    };
                    actions = actions.child(
                        Button::new("header-connect-all")
                            .small()
                            .w(size)
                            .h(size)
                            .icon(
                                Icon::empty()
                                    .path(format!("icons/{conn_icon}.svg"))
                                    .size(px(13.))
                                    .text_color(conn_color),
                            )
                            .tooltip(conn_label.to_owned())
                            .accessibility_label(conn_label)
                            .on_click(
                                cx.listener(|list, _, window, cx| list.action(Action::Connect, None, window, cx)),
                            ),
                    );
                }
                actions = actions.child(
                    Button::new("header-add")
                        .small()
                        .w(size)
                        .h(size)
                        .icon(Icon::empty().path("icons/plus.svg").size(px(13.)))
                        .tooltip(add_label.clone())
                        .accessibility_label(add_label)
                        .on_click(cx.listener(|list, _, window, cx| list.action(Action::Add, None, window, cx))),
                );
                return Some(actions.into_any_element());
            }
            return Some(
                Button::new("header-add")
                    .small()
                    .h(size)
                    .flex_shrink_0()
                    .label(add_label.clone())
                    .accessibility_label(add_label)
                    .icon(Action::Add.icon(&self.model))
                    .on_click(cx.listener(|list, _, window, cx| list.action(Action::Add, None, window, cx)))
                    .into_any_element(),
            );
        }
        (!top.is_empty()).then(|| {
            div()
                .flex()
                .items_center()
                .gap(gap)
                .flex_shrink_0()
                .children(top)
                .into_any_element()
        })
    }
    /// The grid of buttons under the table (a page may draw it later, after another section).
    pub fn button_rows(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        if self.is_connections() || self.is_cards() || self.truly_empty(cx) {
            return None;
        }
        let selected = self.model["rows"].as_array().and_then(|rows| {
            rows.iter()
                .position(|row| !self.selected_id.is_empty() && row["id"] == self.selected_id)
        });
        let actions: Vec<Action> = Action::base_actions(&self.model)
            .into_iter()
            .filter(|action| !self.on_top(*action))
            .collect();
        let per_row = self.model["perRow"].as_u64().unwrap_or(4).max(1) as usize;
        let mut buttons = div().flex().flex_col().gap(px(SPACE_SM)).w_full();
        if self.model["headerAdd"] == true && self.has_connection() {
            if self.model["rows"].as_array().is_none_or(Vec::is_empty) {
                return None;
            }
            let room = self.available_width(window, cx);
            let fold = room < 360.;
            let mut line = div().flex().flex_wrap().items_center().gap(px(SPACE_SM)).w_full();
            for action in actions.iter().copied() {
                if matches!(action, Action::Delete) {
                    line = line.child(div().flex_1());
                }
                let label = action.row_label(&self.model, selected);
                line = line.child(
                    Button::new(SharedString::from(format!("conn-{}", action.id(&self.model))))
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .when(!fold, |button| button.label(label.clone()))
                        .when(fold, |button| button.w(px(34.)).tooltip(label.clone()))
                        .accessibility_label(label)
                        .icon(action.row_icon(&self.model, selected))
                        .disabled(!action.enabled(&self.model, selected))
                        .on_click(cx.listener(move |list, _, window, cx| list.action(action, None, window, cx))),
                );
            }
            return (!actions.is_empty()).then_some(buttons.child(line));
        }
        // A strip: what is done with the selected row, as equal icon buttons on one line (their names
        // on hover and in the right-click menu). What would undo work (Delete) stands apart, last.
        if self.model["strip"] == true {
            let visible: Vec<Action> = actions
                .into_iter()
                .filter(|action| !self.hide_disabled(*action) || action.enabled(&self.model, selected))
                .collect();
            // More buttons than fit on a narrow window go on a second line instead of past its edge.
            let mut line = div().flex().flex_wrap().items_center().gap(px(SPACE_SM)).w_full();
            let mut apart = false;
            for action in visible.iter().copied() {
                let danger = matches!(action, Action::Delete)
                    || matches!(action, Action::Custom(index) if self.model["actions"][index]["danger"] == true);
                if danger && !apart {
                    apart = true;
                    line = line.child(div().flex_1());
                }
                let label = action.label(&self.model);
                line = line.child(
                    Button::new(SharedString::from(format!("strip-{}", action.id(&self.model))))
                        .small()
                        .w(px(40.))
                        .h(px(CONTROL_HEIGHT))
                        .flex_shrink_0()
                        .tooltip(action.tip(&self.model).unwrap_or_else(|| label.clone()))
                        .accessibility_label(label)
                        .icon(action.icon(&self.model))
                        .disabled(!action.enabled(&self.model, selected))
                        .on_click(cx.listener(move |list, _, window, cx| list.action(action, None, window, cx))),
                );
            }
            return (!visible.is_empty()).then_some(buttons.child(line));
        }
        // Buttons fold to their icons when a label no longer fits (docs/UI.md): all of them together.
        let room = if self.model["detailSide"] == true {
            if let Some(dw) = self.model["dialogWidth"].as_f64() {
                super::dialog::fit_width(dw as f32, window) - 2. * crate::theme::SPACE_LG
            } else {
                f32::from(window.viewport_size().width) - 200.
            }
        } else {
            self.available_width(window, cx)
        };
        let fold = actions.chunks(per_row).any(|chunk| {
            let wanted: f32 = chunk.iter().map(|action| label_width(&action.label(&self.model))).sum();
            wanted + 8. * chunk.len() as f32 > room
        });
        // Folded to icons, they all fit on one line.
        let per_row = if fold { actions.len().max(1) } else { per_row };
        for chunk in actions.chunks(per_row) {
            let mut line = div().flex().gap(px(SPACE_SM)).w_full();
            for action in chunk.iter().copied() {
                let id = match action {
                    Action::Custom(index) => SharedString::from(format!("custom-{index}")),
                    _ => SharedString::from(action.id(&self.model)),
                };
                // Each button in a cell of its own: every cell of every row is as wide as the others.
                line = line.child(
                    div().flex_1().min_w_0().child(
                        Button::new(id)
                            .small()
                            .h(px(CONTROL_HEIGHT))
                            .w_full()
                            .when(!fold, |button| button.label(action.label(&self.model)))
                            .when_some(
                                action
                                    .tip(&self.model)
                                    .or_else(|| fold.then(|| action.label(&self.model))),
                                |button, tip| button.tooltip(tip),
                            )
                            .accessibility_label(action.label(&self.model))
                            .icon(action.icon(&self.model))
                            .disabled(!action.enabled(&self.model, selected))
                            .on_click(cx.listener(move |list, _, window, cx| list.action(action, None, window, cx))),
                    ),
                );
            }
            // A short last row keeps the width of the others (buttons are laid out on a grid).
            for _ in chunk.len()..per_row.min(actions.len()) {
                line = line.child(div().flex_1().min_w_0());
            }
            buttons = buttons.child(line);
        }
        // A list whose rows open on a click and has only that button (Details) draws none.
        let only_open = self.model["open"].is_string() && actions.len() == 1;
        (!actions.is_empty() && !only_open).then_some(buttons)
    }
    /// The button grid as its own view, for a list whose buttons come last on the page.
    pub fn buttons_view(&self) -> Entity<ButtonRows> {
        self.rows_view.clone()
    }
}
/// The buttons of one list, drawn where the page puts them.
pub struct ButtonRows(WeakEntity<List>);
impl Render for ButtonRows {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.0.upgrade();
        match list.and_then(|list| list.update(cx, |list, cx| list.button_rows(window, cx))) {
            Some(rows) => rows.into_any_element(),
            None => div().into_any_element(),
        }
    }
}
/// A list's page-wide buttons, drawn in the page's header.
pub struct HeaderActions(WeakEntity<List>);
impl Render for HeaderActions {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.0.upgrade();
        match list.and_then(|list| {
            list.update(cx, |list, cx| {
                let top = if list.header_add() {
                    Vec::new()
                } else {
                    list.top_buttons(0., window, cx)
                };
                list.heading_actions(list.header_add(), top, cx)
            })
        }) {
            Some(actions) => actions,
            None => div().into_any_element(),
        }
    }
}
impl List {
    /// What pressing `F2` triggers on the selected row: an explicit `rename` action first, or `Edit`.
    fn f2_action(shown: &Value, row: usize) -> Option<Action> {
        let actions = Action::base_actions(shown);
        let rename = actions
            .iter()
            .copied()
            .find(|action| matches!(action, Action::Custom(index) if shown["actions"][*index]["id"] == "rename"))
            .filter(|action| action.enabled(shown, Some(row)));
        rename.or_else(|| {
            actions
                .into_iter()
                .find(|action| matches!(action, Action::Edit))
                .filter(|action| action.enabled(shown, Some(row)))
        })
    }
}
impl Render for List {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hugs = self.hugs();
        let mut body = div()
            .w_full()
            .when(!self.is_connections() && !self.is_cards() && !hugs, |el| el.h_full())
            .flex()
            .flex_col()
            .gap(px(SPACE_SM))
            .on_key_down(cx.listener(|list, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != "f2"
                    || event.keystroke.modifiers.control
                    || event.keystroke.modifiers.alt
                    || event.keystroke.modifiers.platform
                    || window.has_active_dialog(cx)
                {
                    return;
                }
                let typing = list
                    .search
                    .as_ref()
                    .is_some_and(|search| search.read(cx).focus_handle(cx).contains_focused(window, cx));
                if typing || list.selected_id.is_empty() {
                    return;
                }
                let shown = match &list.table {
                    Some(table) => table.read(cx).delegate().model.clone(),
                    None => list.model.clone(),
                };
                let Some(row) = shown["rows"]
                    .as_array()
                    .and_then(|rows| rows.iter().position(|row| row["id"] == list.selected_id))
                else {
                    return;
                };
                if let Some(action) = Self::f2_action(&shown, row) {
                    list.action(action, Some(row), window, cx);
                    cx.stop_propagation();
                }
            }));
        let mut stack = Stack::new();
        let guide = self.model["guide"].as_str().unwrap_or_default();
        let options = self.model["filter"]["options"].as_array().cloned().unwrap_or_default();
        let truly_empty = self.empty_state(cx);
        // The "Show" bar goes on the toolbar's line when it fits beside what else is there.
        let reserved = self.show.as_ref().map_or(0., |tabs| {
            let room = self.available_width(window, cx);
            let wanted = super::tabs::inline_width(tabs) + SPACE_SM;
            let buttons = if truly_empty { 0 } else { self.top_actions().len() };
            let others =
                if self.search.is_some() { 180. } else { 0. } + (buttons + options.len()) as f32 * (34. + SPACE_SM);
            if wanted + others <= room {
                wanted
            } else {
                0.
            }
        });
        let mut top = if truly_empty {
            Vec::new()
        } else {
            self.top_buttons(reserved, window, cx)
        };
        let show_heading = self.heading_visible();
        let header_add = self.header_add();
        let header_top = show_heading
            && !header_add
            && self.search.is_none()
            && options.is_empty()
            && self.show.is_none()
            && !top.is_empty();
        // The page's header carries these buttons instead of the toolbar.
        if self.hosts_actions(cx) && !header_add {
            top.clear();
        }
        if show_heading {
            let row_count = self.model["rows"].as_array().map_or(0, Vec::len);
            let has_rows = row_count > 0;
            let mut heading = div()
                .id("list-heading")
                .test_support()
                .role(Role::Heading)
                .aria_label(self.model["title"].as_str().unwrap_or_default().to_owned())
                .flex()
                .items_center()
                .gap(px(SPACE_SM))
                .min_h(px(if header_add || header_top {
                    CONTROL_HEIGHT
                } else {
                    crate::theme::LINE_SECTION_TITLE
                }))
                .when(header_add || header_top, |row| row.justify_between().w_full());
            let mut left = div()
                .flex()
                .items_center()
                .gap(px(crate::theme::SPACE_SM))
                .min_w_0()
                .font_weight(FontWeight::BOLD)
                .text_size(px(crate::theme::FONT_SECTION_TITLE))
                .line_height(px(crate::theme::LINE_SECTION_TITLE))
                .text_color(crate::theme::strong(cx))
                .when_some(self.model["brand"].as_str(), |title, brand| {
                    title.child(super::body::brand_logo("title-brand", brand, px(16.), cx))
                })
                .when_some(self.model["icon"].as_str(), |title, icon| {
                    title.child(
                        gpui_kit::component::Icon::empty()
                            .path(format!("icons/{icon}.svg"))
                            .small(),
                    )
                })
                .child(self.model["title"].as_str().unwrap_or_default().to_owned());
            if header_add && self.has_connection() && has_rows {
                let (icon, color) =
                    super::health::appearance(self.model["connection"]["health"].as_str().unwrap_or_default(), cx);
                left = left.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(SPACE_XS))
                        .px(px(SPACE_SM))
                        .py(px(SPACE_XS))
                        .rounded(px(99.))
                        .bg(cx.theme().sidebar)
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_size(px(FONT_CAPTION))
                        .line_height(px(LINE_CAPTION))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(color)
                        .child(Icon::empty().path(format!("icons/{icon}.svg")).size(px(12.)))
                        .child(self.model["connection"]["text"].as_str().unwrap_or_default().to_owned()),
                );
            }
            heading = heading.child(left);
            if let Some(actions) = self.heading_actions(header_add, std::mem::take(&mut top), cx) {
                heading = heading.child(actions);
            }
            // A section's own note goes straight under its heading, 4px; with none, 8px to its content.
            let has_note = (!self.lead_outside && self.intro_with(true, cx).is_some())
                || (!guide.is_empty() && self.model["guideBelow"] != true);
            stack.add(heading, if has_note { SPACE_XS } else { SPACE_SM });
        }
        // A page's introduction and its state line, under its section heading when not hoisted above sections.
        // The page's own guide has 12 under it; a section's note, 8.
        let after_guide = if show_heading { SPACE_SM } else { SPACE_MD };
        if !self.lead_outside {
            if let Some(lead) = self.intro_with(show_heading, cx) {
                stack.add(lead, after_guide);
            }
        }
        let guide_below = self.model["guideBelow"] == true;
        if !guide.is_empty() && !guide_below {
            stack.add(
                if show_heading {
                    super::notice::note(guide, cx)
                } else {
                    super::page::guide(guide, cx)
                },
                after_guide,
            );
        }
        if let Some(err) = &self.error {
            stack.add(super::notice::error(err, cx), SPACE_SM);
        }
        if let Some(bar) = Self::progress_bar(&self.model["progress"], cx) {
            stack.add(bar, SPACE_SM);
        }
        if let (true, Some(inline)) = (self.is_settings(), self.inline.clone()) {
            stack.add(inline, SPACE_SM);
        } else if self.is_settings() {
            body = body.child(stack.column());
            for (index, row) in self.model["rows"].as_array().into_iter().flatten().enumerate() {
                let health_str = row["health"].as_str().unwrap_or("off");
                let name = row["name"].as_str().unwrap_or_default().to_owned();
                let enabled = row["enabled"].as_bool().unwrap_or(health_str == "up");
                let location = row["location"].as_str().unwrap_or_default().to_owned();
                let proto_icon = row["icon"].as_str().unwrap_or("server");
                let has_auth = row["auth"].as_bool().unwrap_or(false);
                let status_icon = if enabled { "power" } else { "power-off" };
                let status_color = if enabled {
                    super::health::appearance("up", cx).1
                } else {
                    crate::theme::quiet(cx)
                };
                let owner = cx.entity().downgrade();
                body = body.child(
                    div()
                        .id(("setting-row", index))
                        .test_support()
                        .role(Role::Group)
                        .aria_label(name.clone())
                        .flex()
                        .items_center()
                        .justify_between()
                        .h(px(CONTROL_HEIGHT))
                        .px(px(SPACE_MD))
                        .rounded(px(6.))
                        .bg(crate::theme::row_surface(cx))
                        .border_1()
                        .border_color(crate::theme::row_border(cx))
                        .hover(|style| style.bg(cx.theme().muted))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_SM))
                                .min_w_0()
                                .flex_1()
                                .child(
                                    Icon::empty()
                                        .path(format!("icons/{proto_icon}.svg"))
                                        .size(px(14.))
                                        .text_color(if enabled {
                                            cx.theme().foreground
                                        } else {
                                            crate::theme::quiet(cx)
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(FONT_BODY))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(if enabled {
                                            cx.theme().foreground
                                        } else {
                                            crate::theme::quiet(cx)
                                        })
                                        .child(name.clone()),
                                )
                                .when(!location.is_empty(), |row_el| {
                                    row_el.child(
                                        div()
                                            .text_size(px(FONT_CAPTION))
                                            .text_color(if enabled {
                                                crate::theme::link_color(cx)
                                            } else {
                                                crate::theme::quiet(cx)
                                            })
                                            .child(location),
                                    )
                                })
                                .when(has_auth, |row_el| {
                                    row_el.child(pill(
                                        ("setting-secret", index),
                                        Some("lock"),
                                        Some(tr("lists.servers.password_tag").trim_start_matches(", ").to_owned()),
                                        cx,
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_SM))
                                .child(
                                    Button::new(("toggle-setting", index))
                                        .small()
                                        .w(px(DENSE_CONTROL_HEIGHT))
                                        .h(px(DENSE_CONTROL_HEIGHT))
                                        .text_color(status_color)
                                        .icon(
                                            Icon::empty()
                                                .path(format!("icons/{status_icon}.svg"))
                                                .size(px(13.))
                                                .text_color(status_color),
                                        )
                                        .tooltip(tr(if enabled {
                                            "lists.servers.enabled"
                                        } else {
                                            "lists.servers.disabled"
                                        }))
                                        .accessibility_label(format!("{}: {name}", tr("btn.on_off")))
                                        .on_click(cx.listener(move |list, _, window, cx| {
                                            list.action(Action::Toggle, Some(index), window, cx)
                                        })),
                                )
                                .child(
                                    Button::new(("edit-setting", index))
                                        .small()
                                        .w(px(DENSE_CONTROL_HEIGHT))
                                        .h(px(DENSE_CONTROL_HEIGHT))
                                        .icon(Icon::empty().path("icons/pencil.svg").size(px(13.)))
                                        .tooltip(tr("btn.edit"))
                                        .accessibility_label(format!("{}: {name}", tr("btn.edit")))
                                        .on_click(cx.listener(move |list, _, window, cx| {
                                            list.action(Action::Edit, Some(index), window, cx)
                                        })),
                                ),
                        )
                        .context_menu(move |menu, _, _| {
                            let toggle_owner = owner.clone();
                            let edit_owner = owner.clone();
                            menu.item(
                                PopupMenuItem::new(tr("btn.on_off"))
                                    .icon(Action::Toggle.icon(&Value::Null))
                                    .on_click(move |_, window, cx| {
                                        let _ = toggle_owner.update(cx, |list, cx| {
                                            list.action(Action::Toggle, Some(index), window, cx)
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(tr("btn.edit"))
                                    .icon(Action::Edit.icon(&Value::Null))
                                    .on_click(move |_, window, cx| {
                                        let _ = edit_owner
                                            .update(cx, |list, cx| list.action(Action::Edit, Some(index), window, cx));
                                    }),
                            )
                        }),
                );
            }
            return body.into_any_element();
        } else if self.is_connections() {
            body = body.child(stack.column());
            let rows = self.model["rows"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                body = body.child(
                    div()
                        .h(px(CONTROL_HEIGHT))
                        .px(px(SPACE_MD))
                        .rounded(px(6.))
                        .bg(crate::theme::row_surface(cx))
                        .border_1()
                        .border_color(crate::theme::row_border(cx))
                        .flex()
                        .items_center()
                        .gap(px(SPACE_SM))
                        .text_size(px(FONT_BODY))
                        .text_color(crate::theme::quiet(cx))
                        .child(Icon::empty().path("icons/plug.svg").size(px(14.)))
                        .child(self.model["empty"].as_str().unwrap_or_default().to_owned()),
                );
                return body.into_any_element();
            }
            for (index, row) in rows.iter().enumerate() {
                let name = row["name"].as_str().unwrap_or_default().to_owned();
                let connected = row["connected"] == true;
                let auto_connect = row["autoConnect"] == true;
                let address = row["address"].as_str().unwrap_or_default().to_owned();
                let reconnect = row["reconnect"] == true;
                let retry_interval = row["retryInterval"].as_u64().unwrap_or(5);
                let secret = row["secret"] == true;
                let is_default = row["default"] == true;
                let brand = row["brand"]
                    .as_str()
                    .or_else(|| self.model["brand"].as_str())
                    .unwrap_or("obsstudio")
                    .to_owned();
                let conn_icon = if connected { "plug" } else { "unplug" };
                let conn_color = if connected {
                    super::health::appearance("up", cx).1
                } else {
                    crate::theme::quiet(cx)
                };
                let conn_label = tr(if connected { "btn.disconnect" } else { "btn.connect" }).to_owned();
                let auto_icon = if auto_connect { "power" } else { "power-off" };
                let auto_color = if auto_connect {
                    super::health::appearance("up", cx).1
                } else {
                    crate::theme::quiet(cx)
                };
                let auto_tip = format!(
                    "{}: {}",
                    tr("lists.on_start"),
                    tr(if auto_connect {
                        "lists.servers.enabled"
                    } else {
                        "lists.servers.disabled"
                    })
                );
                let secret_tip = tr(if self.kind() == "obs_connection" {
                    "lists.password"
                } else {
                    "lists.slobs.token"
                })
                .to_owned();
                let retry_tip = tr("lists.retry_every").replace("{seconds}", &retry_interval.to_string());
                let owner = cx.entity().downgrade();
                let conn_label_menu = conn_label.clone();
                body = body.child(
                    div()
                        .id(("conn-row", index))
                        .test_support()
                        .role(Role::Group)
                        .aria_label(name.clone())
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(SPACE_SM))
                        .h(px(CONTROL_HEIGHT))
                        .px(px(SPACE_MD))
                        .rounded(px(6.))
                        .bg(crate::theme::row_surface(cx))
                        .border_1()
                        .border_color(crate::theme::row_border(cx))
                        .hover(|style| style.bg(crate::theme::row_surface_hover(cx)))
                        .child(
                            div()
                                .id(("conn-info", index))
                                .flex()
                                .items_center()
                                .gap(px(SPACE_SM))
                                .min_w_0()
                                .flex_1()
                                .on_click(cx.listener(move |list, event: &ClickEvent, window, cx| {
                                    if event.click_count() > 1 {
                                        list.action(Action::Edit, Some(index), window, cx);
                                    }
                                }))
                                .child(super::body::brand_logo(("conn-brand", index), &brand, px(16.), cx))
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(FONT_BODY))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(cx.theme().foreground)
                                        .child(name.clone()),
                                )
                                .when(!address.is_empty(), |row_el| {
                                    row_el.child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_size(px(FONT_CAPTION))
                                            .text_color(if connected {
                                                crate::theme::link_color(cx)
                                            } else {
                                                crate::theme::quiet(cx)
                                            })
                                            .child(address),
                                    )
                                })
                                .when(secret, |row_el| {
                                    row_el.child(pill(("conn-secret", index), Some("lock"), None, cx).tooltip(
                                        move |window, cx| {
                                            gpui_kit::component::tooltip::Tooltip::new(secret_tip.clone())
                                                .build(window, cx)
                                        },
                                    ))
                                })
                                .when(reconnect, |row_el| {
                                    let tip = retry_tip.clone();
                                    row_el.child(
                                        pill(
                                            ("conn-retry", index),
                                            Some("refresh-cw"),
                                            Some(format!("{retry_interval}s")),
                                            cx,
                                        )
                                        .tooltip(move |window, cx| {
                                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                        }),
                                    )
                                })
                                .when(is_default, |row_el| {
                                    row_el.child(pill(("conn-default", index), Some("star-filled"), None, cx).tooltip(
                                        move |window, cx| {
                                            gpui_kit::component::tooltip::Tooltip::new(
                                                tr("lists.default_badge").to_owned(),
                                            )
                                            .build(window, cx)
                                        },
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_SM))
                                .flex_shrink_0()
                                .child(
                                    Button::new(("conn-toggle", index))
                                        .small()
                                        .w(px(DENSE_CONTROL_HEIGHT))
                                        .h(px(DENSE_CONTROL_HEIGHT))
                                        .text_color(conn_color)
                                        .icon(
                                            Icon::empty()
                                                .path(format!("icons/{conn_icon}.svg"))
                                                .size(px(13.))
                                                .text_color(conn_color),
                                        )
                                        .tooltip(conn_label.clone())
                                        .accessibility_label(format!("{conn_label}: {name}"))
                                        .on_click(cx.listener(move |list, _, window, cx| {
                                            list.action(Action::Connect, Some(index), window, cx)
                                        })),
                                )
                                .child(
                                    Button::new(("conn-autostart", index))
                                        .small()
                                        .w(px(DENSE_CONTROL_HEIGHT))
                                        .h(px(DENSE_CONTROL_HEIGHT))
                                        .text_color(auto_color)
                                        .icon(
                                            Icon::empty()
                                                .path(format!("icons/{auto_icon}.svg"))
                                                .size(px(13.))
                                                .text_color(auto_color),
                                        )
                                        .tooltip(auto_tip)
                                        .accessibility_label(format!("{}: {name}", tr("lists.on_start")))
                                        .on_click(cx.listener(move |list, _, window, cx| {
                                            list.action(Action::Toggle, Some(index), window, cx)
                                        })),
                                )
                                .child(
                                    Button::new(("conn-edit", index))
                                        .small()
                                        .w(px(DENSE_CONTROL_HEIGHT))
                                        .h(px(DENSE_CONTROL_HEIGHT))
                                        .icon(Icon::empty().path("icons/pencil.svg").size(px(13.)))
                                        .tooltip(tr("btn.edit"))
                                        .accessibility_label(format!("{}: {name}", tr("btn.edit")))
                                        .on_click(cx.listener(move |list, _, window, cx| {
                                            list.action(Action::Edit, Some(index), window, cx)
                                        })),
                                )
                                .child(
                                    Button::new(("conn-delete", index))
                                        .small()
                                        .w(px(DENSE_CONTROL_HEIGHT))
                                        .h(px(DENSE_CONTROL_HEIGHT))
                                        .icon(
                                            Icon::empty()
                                                .path("icons/trash-2.svg")
                                                .size(px(13.))
                                                .text_color(crate::theme::quiet(cx)),
                                        )
                                        .tooltip(tr("btn.delete"))
                                        .accessibility_label(format!("{}: {name}", tr("btn.delete")))
                                        .on_click(cx.listener(move |list, _, window, cx| {
                                            list.action(Action::Delete, Some(index), window, cx)
                                        })),
                                ),
                        )
                        .context_menu(move |menu, _, _| {
                            let conn_owner = owner.clone();
                            let auto_owner = owner.clone();
                            let edit_owner = owner.clone();
                            let delete_owner = owner.clone();
                            menu.item(
                                PopupMenuItem::new(conn_label_menu.clone())
                                    .icon(Icon::empty().path(format!("icons/{conn_icon}.svg")))
                                    .on_click(move |_, window, cx| {
                                        let _ = conn_owner.update(cx, |list, cx| {
                                            list.action(Action::Connect, Some(index), window, cx)
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(tr("lists.on_start"))
                                    .icon(Icon::empty().path(format!("icons/{auto_icon}.svg")))
                                    .on_click(move |_, window, cx| {
                                        let _ = auto_owner.update(cx, |list, cx| {
                                            list.action(Action::Toggle, Some(index), window, cx)
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(tr("btn.edit"))
                                    .icon(Action::Edit.icon(&Value::Null))
                                    .on_click(move |_, window, cx| {
                                        let _ = edit_owner
                                            .update(cx, |list, cx| list.action(Action::Edit, Some(index), window, cx));
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(tr("btn.delete"))
                                    .icon(Action::Delete.icon(&Value::Null))
                                    .on_click(move |_, window, cx| {
                                        let _ = delete_owner.update(cx, |list, cx| {
                                            list.action(Action::Delete, Some(index), window, cx)
                                        });
                                    }),
                            )
                        }),
                );
            }
            return body.into_any_element();
        } else if self.is_cards() {
            body = body.child(stack.column());
            let rows = self.model["rows"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                let empty_icon = self.model["icon"].as_str().unwrap_or("folder");
                let empty_model = self.model.clone();
                let empty_owner = cx.entity().downgrade();
                body = body.child(
                    div()
                        .id("cards-empty")
                        .h(px(CONTROL_HEIGHT))
                        .px(px(SPACE_MD))
                        .rounded(px(6.))
                        .bg(crate::theme::row_surface(cx))
                        .border_1()
                        .border_color(crate::theme::row_border(cx))
                        .flex()
                        .items_center()
                        .gap(px(SPACE_SM))
                        .text_size(px(FONT_BODY))
                        .text_color(crate::theme::quiet(cx))
                        .child(Icon::empty().path(format!("icons/{empty_icon}.svg")).size(px(14.)))
                        .child(self.model["empty"].as_str().unwrap_or_default().to_owned())
                        .context_menu(move |menu, _, _| {
                            Action::actions(&empty_model).into_iter().fold(menu, |menu, action| {
                                let menu_owner = empty_owner.clone();
                                menu.item(
                                    PopupMenuItem::new(action.row_label(&empty_model, None))
                                        .icon(action.row_icon(&empty_model, None))
                                        .disabled(!action.enabled(&empty_model, None))
                                        .on_click(move |_, window, cx| {
                                            let _ =
                                                menu_owner.update(cx, |list, cx| list.action(action, None, window, cx));
                                        }),
                                )
                            })
                        }),
                );
                return body.into_any_element();
            }
            let base_actions = Action::base_actions(&self.model);
            let has_toggle = base_actions.iter().any(|a| matches!(a, Action::Toggle));
            let has_edit = base_actions.iter().any(|a| matches!(a, Action::Edit));
            let has_delete = base_actions.iter().any(|a| matches!(a, Action::Delete));
            for (index, row) in rows.iter().enumerate() {
                let name = row["name"].as_str().unwrap_or_default().to_owned();
                let enabled = row["enabled"].as_bool().unwrap_or(true);
                let detail = row["detail"].as_str().unwrap_or_default().to_owned();
                let row_icon = row["icon"]
                    .as_str()
                    .or_else(|| self.model["icon"].as_str())
                    .unwrap_or("circle")
                    .to_owned();
                let health = row["health"].as_str().map(str::to_owned);
                let health_tip = row["healthTip"].as_str().unwrap_or_default().to_owned();
                let switch_only = row["switchOnly"] == true;
                let badges = row["badges"].as_array().cloned().unwrap_or_default();
                let toggle_icon = if enabled { "power" } else { "power-off" };
                let toggle_color = if enabled {
                    super::health::appearance("up", cx).1
                } else {
                    crate::theme::quiet(cx)
                };
                let toggle_tip = tr(if enabled {
                    "lists.servers.enabled"
                } else {
                    "lists.servers.disabled"
                })
                .to_owned();
                let auto_tip = format!(
                    "{}: {}",
                    tr("lists.on_start"),
                    tr(if enabled {
                        "lists.servers.enabled"
                    } else {
                        "lists.servers.disabled"
                    })
                );
                let owner = cx.entity().downgrade();
                let menu_model = self.model.clone();
                let mut info = div()
                    .id(("card-info", index))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_SM))
                    .min_w_0()
                    .flex_1()
                    .on_click(cx.listener(move |list, event: &ClickEvent, window, cx| {
                        if event.click_count() > 1 {
                            list.action(Action::Edit, Some(index), window, cx);
                        }
                    }));
                if let Some(health_str) = health {
                    let (h_icon, h_color) = super::health::appearance(&health_str, cx);
                    let mut h_el = div()
                        .id(("card-health", index))
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .child(
                            Icon::empty()
                                .path(format!("icons/{h_icon}.svg"))
                                .size(px(14.))
                                .text_color(h_color),
                        );
                    if !health_tip.is_empty() {
                        let tip = health_tip.clone();
                        h_el = h_el.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        });
                    }
                    info = info.child(h_el);
                } else {
                    info = info.child(
                        Icon::empty()
                            .path(format!("icons/{row_icon}.svg"))
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(if enabled {
                                cx.theme().foreground
                            } else {
                                crate::theme::quiet(cx)
                            }),
                    );
                }
                info = info
                    .child(
                        div()
                            .max_w(px(200.))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(FONT_BODY))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if enabled {
                                cx.theme().foreground
                            } else {
                                crate::theme::quiet(cx)
                            })
                            .child(name.clone()),
                    )
                    .when(!detail.is_empty(), |row_el| {
                        let detail_tip = detail.clone();
                        row_el.child(
                            div()
                                .id(("card-detail", index))
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(FONT_CAPTION))
                                .text_color(if enabled {
                                    crate::theme::link_color(cx)
                                } else {
                                    crate::theme::quiet(cx)
                                })
                                .child(detail)
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(detail_tip.clone()).build(window, cx)
                                }),
                        )
                    });
                for (b_idx, badge) in badges.into_iter().enumerate() {
                    let b_icon = badge["icon"].as_str().unwrap_or_default().to_owned();
                    let b_text = badge["text"].as_str().unwrap_or_default().to_owned();
                    let b_tip = badge["tip"].as_str().unwrap_or_default().to_owned();
                    if b_icon.is_empty() && b_text.is_empty() {
                        continue;
                    }
                    let mut tag = pill(
                        ("card-badge", index.wrapping_mul(32).wrapping_add(b_idx)),
                        (!b_icon.is_empty()).then_some(b_icon.as_str()),
                        (!b_text.is_empty()).then_some(b_text),
                        cx,
                    );
                    if !b_tip.is_empty() {
                        let tip = b_tip.clone();
                        tag = tag.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        });
                    }
                    info = info.child(tag);
                }
                let mut right = div().flex().items_center().gap(px(SPACE_SM)).flex_shrink_0();
                if switch_only {
                    right = right.child(
                        div()
                            .id(("card-switch-tip", index))
                            .flex()
                            .items_center()
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(auto_tip.clone()).build(window, cx)
                            })
                            .child(
                                Switch::new(("card-switch", index))
                                    .small()
                                    .checked(enabled)
                                    .disabled(true)
                                    .accessibility_label(name.clone()),
                            ),
                    );
                } else if has_toggle {
                    right = right.child(
                        Button::new(("card-toggle", index))
                            .small()
                            .w(px(DENSE_CONTROL_HEIGHT))
                            .h(px(DENSE_CONTROL_HEIGHT))
                            .text_color(toggle_color)
                            .icon(
                                Icon::empty()
                                    .path(format!("icons/{toggle_icon}.svg"))
                                    .size(px(13.))
                                    .text_color(toggle_color),
                            )
                            .tooltip(toggle_tip)
                            .accessibility_label(tr("btn.on_off"))
                            .disabled(row["can"]["toggle"] == false)
                            .on_click(cx.listener(move |list, _, window, cx| {
                                list.action(Action::Toggle, Some(index), window, cx)
                            })),
                    );
                }
                if has_edit {
                    right = right.child(
                        Button::new(("card-edit", index))
                            .small()
                            .w(px(DENSE_CONTROL_HEIGHT))
                            .h(px(DENSE_CONTROL_HEIGHT))
                            .icon(Icon::empty().path("icons/pencil.svg").size(px(13.)))
                            .tooltip(tr("btn.edit"))
                            .accessibility_label(tr("btn.edit"))
                            .disabled(row["can"]["edit"] == false)
                            .on_click(cx.listener(move |list, _, window, cx| {
                                list.action(Action::Edit, Some(index), window, cx)
                            })),
                    );
                }
                if has_delete {
                    right = right.child(
                        Button::new(("card-delete", index))
                            .small()
                            .w(px(DENSE_CONTROL_HEIGHT))
                            .h(px(DENSE_CONTROL_HEIGHT))
                            .icon(
                                Icon::empty()
                                    .path("icons/trash-2.svg")
                                    .size(px(13.))
                                    .text_color(crate::theme::quiet(cx)),
                            )
                            .tooltip(tr("btn.delete"))
                            .accessibility_label(tr("btn.delete"))
                            .disabled(row["can"]["delete"] == false)
                            .on_click(cx.listener(move |list, _, window, cx| {
                                list.action(Action::Delete, Some(index), window, cx)
                            })),
                    );
                }
                body = body.child(
                    div()
                        .id(("card-row", index))
                        .test_support()
                        .role(Role::Group)
                        .aria_label(name)
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(SPACE_SM))
                        .h(px(CONTROL_HEIGHT))
                        .px(px(SPACE_MD))
                        .rounded(px(6.))
                        .bg(crate::theme::row_surface(cx))
                        .border_1()
                        .border_color(crate::theme::row_border(cx))
                        .hover(|style| style.bg(crate::theme::row_surface_hover(cx)))
                        .child(info)
                        .child(right)
                        .context_menu(move |menu, _, _| {
                            Action::actions(&menu_model).into_iter().fold(menu, |menu, action| {
                                let menu_owner = owner.clone();
                                menu.item(
                                    PopupMenuItem::new(action.row_label(&menu_model, Some(index)))
                                        .icon(action.row_icon(&menu_model, Some(index)))
                                        .disabled(!action.enabled(&menu_model, Some(index)))
                                        .on_click(move |_, window, cx| {
                                            let _ = menu_owner
                                                .update(cx, |list, cx| list.action(action, Some(index), window, cx));
                                        }),
                                )
                            })
                        }),
                );
            }
            return body.into_any_element();
        }
        if self.search.is_some() || !options.is_empty() || self.show.is_some() || !top.is_empty() {
            let mut line = div()
                .flex()
                .items_center()
                .gap(px(SPACE_SM))
                .w_full()
                .min_h(px(CONTROL_HEIGHT));
            if let Some(tabs) = self.show.clone() {
                let owner = cx.entity().downgrade();
                let tab_bar = super::tabs::bar(&tabs, window, cx, move |id, _, cx| {
                    let _ = owner.update(cx, |_, cx| cx.emit(ViewChange::Tab(id)));
                });
                // The bar shares the toolbar's line when it fits beside the search (or the buttons), else
                // it has a line of its own.
                if reserved > 0. || (self.search.is_none() && top.is_empty() && options.is_empty()) {
                    line = line.child(tab_bar);
                } else {
                    stack.add(tab_bar, SPACE_SM);
                }
            }
            if let Some(search) = &self.search {
                line = line.child(
                    div().flex_1().min_w_0().h(px(CONTROL_HEIGHT)).child(
                        Input::new(search)
                            .prefix(Icon::empty().path("icons/search.svg").size_3p5())
                            .small()
                            .min_h(px(CONTROL_HEIGHT))
                            .bg(cx.theme().table)
                            .text_size(px(FONT_BODY))
                            .aria_label(
                                self.model["searchHint"]
                                    .as_str()
                                    .map_or_else(|| tr("lists.search").to_owned(), str::to_owned),
                            ),
                    ),
                );
            }
            // Filter chips beside Search: brand logos (platforms) or icon+label toggles (catalog filters).
            let toggles = self.model["filter"]["mode"] == "toggles";
            let fold_filters = options
                .iter()
                .any(|option| option["brand"].as_str().unwrap_or_default().is_empty())
                && {
                    let filter_w: f32 = options
                        .iter()
                        .map(|option| {
                            if !option["brand"].as_str().unwrap_or_default().is_empty() {
                                40.
                            } else {
                                label_width(option["label"].as_str().unwrap_or_default())
                            }
                        })
                        .sum();
                    let top_w = top.len() as f32 * (34. + SPACE_SM);
                    let search_min = if self.search.is_some() { 160. } else { 0. };
                    let room = self.available_width(window, cx) - reserved;
                    filter_w + 8. * options.len() as f32 + search_min + top_w > room
                };
            for (index, option) in options.iter().enumerate() {
                let label = option["label"].as_str().unwrap_or_default().to_owned();
                let tip = option["tip"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| label.clone());
                let shown = option["shown"] == true;
                let brand = option["brand"].as_str().unwrap_or_default();
                let btn = if !brand.is_empty() {
                    Button::new(("filter", index))
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .when(shown, |button| button.outline())
                        .when(!shown, |button| button.ghost().opacity(0.4))
                        .tooltip(tip)
                        .accessibility_label(label)
                        .child(super::body::brand_logo(("filter-logo", index), brand, px(16.), cx))
                        .on_click(cx.listener(move |list, _, _, cx| list.toggle_filter(index, cx)))
                } else {
                    let icon = option["icon"].as_str().unwrap_or("filter");
                    Button::new(("filter", index))
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .flex_shrink_0()
                        .when(toggles && shown, |button| button.primary())
                        .when(!toggles && shown, |button| button.outline())
                        .when(!shown, |button| button.outline().opacity(0.65))
                        .icon(Icon::empty().path(format!("icons/{icon}.svg")))
                        .when(!fold_filters, |button| button.label(label.clone()))
                        .when(fold_filters, |button| button.w(px(34.)))
                        .tooltip(tip)
                        .accessibility_label(label)
                        .on_click(cx.listener(move |list, _, _, cx| list.toggle_filter(index, cx)))
                };
                line = line.child(btn);
            }
            if self.search.is_none() && !top.is_empty() {
                line = line.child(div().flex_1());
            }
            stack.add(line.children(top), SPACE_SM);
        }
        let selected = self.model["rows"].as_array().and_then(|rows| {
            rows.iter()
                .position(|row| !self.selected_id.is_empty() && row["id"] == self.selected_id)
        });
        let detail_side = self.model["detailSide"] == true;
        if truly_empty {
            let actions = Action::base_actions(&self.model);
            let mut choices = div().flex().flex_wrap().items_center().gap(px(SPACE_SM));
            for action in actions.into_iter().filter(|action| action.enabled(&self.model, None)) {
                let label = action.label(&self.model);
                choices = choices.child(
                    Button::new(SharedString::from(format!("empty-{}", action.id(&self.model))))
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .icon(action.icon(&self.model))
                        .label(label)
                        .when(matches!(action, Action::Add)
                            || matches!(action, Action::Custom(index) if self.model["actions"][index]["primary"] == true),
                            |button| button.primary())
                        .on_click(cx.listener(move |list, _, window, cx| list.action(action, None, window, cx))),
                );
            }
            let owner = cx.entity().downgrade();
            let model = self.model.clone();
            stack.add(
                div()
                    .id("empty-list")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_SM))
                    .child(super::notice::info(
                        self.model["empty"].as_str().unwrap_or_default(),
                        cx,
                    ))
                    .child(choices)
                    .context_menu(move |menu, _, _| {
                        Action::actions(&model).into_iter().fold(menu, |menu, action| {
                            let owner = owner.clone();
                            menu.item(
                                PopupMenuItem::new(action.label(&model))
                                    .icon(action.icon(&model))
                                    .disabled(!action.enabled(&model, None))
                                    .on_click(move |_, window, cx| {
                                        let _ = owner.update(cx, |list, cx| list.action(action, None, window, cx));
                                    }),
                            )
                        })
                    }),
                SPACE_SM,
            );
        } else if let Some(table) = &self.table {
            // A table sized to its rows (a few at least, `tableRows` at most) when it shares the page
            // with other sections or with a detail; the only one on its page takes what is left.
            let shown = table.read(cx).delegate().model["rows"].as_array().map_or(0, Vec::len);
            let available_w = if let Some(dw) = self.model["dialogWidth"].as_f64() {
                super::dialog::fit_width(dw as f32, window) - 2. * crate::theme::SPACE_LG
            } else {
                f32::from(window.viewport_size().width) - 220.
            };
            let side_by_side = detail_side && available_w >= 600.;
            let size_to_rows = hugs || self.model["tableRows"].is_u64();
            let most = self.model["tableRows"]
                .as_u64()
                .unwrap_or(if detail_side && !side_by_side {
                    5
                } else {
                    MOST_ROWS as u64
                }) as usize;
            let height = (!side_by_side && size_to_rows).then(|| table_height(shown, most));
            let table_entity = table.clone();
            let list_entity = cx.entity().downgrade();
            let table_box = div()
                .relative()
                .when(!side_by_side, |wrap| wrap.w_full())
                .when(side_by_side, |wrap| wrap.w(px(260.)).h_full().flex_shrink_0())
                .min_w_0()
                .when_some(height, |wrap, height| wrap.h(px(height)).flex_shrink_0())
                .when(!side_by_side && height.is_none(), |wrap| wrap.flex_1().min_h_0())
                .on_prepaint(move |bounds, window, cx| {
                    let seen = f32::from(bounds.size.width);
                    let fit = table_entity.read(cx).delegate().fit;
                    if seen > 1. && (seen - fit).abs() > 1. {
                        window.defer(cx, move |_, cx| {
                            let changed = table_entity.update(cx, |table, cx| {
                                if (seen - table.delegate().fit).abs() > 1. {
                                    table.delegate_mut().fit = seen;
                                    table.delegate_mut().layout();
                                    table.refresh(cx);
                                    cx.notify();
                                    true
                                } else {
                                    false
                                }
                            });
                            if changed {
                                let _ = list_entity.update(cx, |_, cx| cx.notify());
                            }
                        });
                    }
                })
                .child(
                    DataTable::new(table)
                        .accessibility_label(self.model["title"].as_str().unwrap_or_default().to_owned())
                        .bordered(false)
                        // Columns shrink to the table's width (rows::fit_widths): never a sideways bar.
                        .scrollbar_visible(true, false)
                        .header_row_height(px(TABLE_HEAD))
                        .with_size(gpui_kit::component::Size::Size(px(CONTROL_HEIGHT))),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(TABLE_HEAD))
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .border_1()
                        .border_color(cx.theme().input)
                        .rounded(px(6.)),
                );
            if detail_side {
                let (foreground, danger, input, table_bg, border, muted_foreground) = {
                    let theme = cx.theme();
                    (
                        theme.foreground,
                        theme.danger,
                        theme.input,
                        theme.table,
                        theme.border,
                        theme.muted_foreground,
                    )
                };
                let row = selected.and_then(|row| self.model["rows"].get(row));
                let mut detail_card = div()
                    .id("detail")
                    .w_full()
                    .when(side_by_side, |card| card.h_full())
                    .when(!side_by_side, |card| {
                        card.min_h(px(88.)).max_h(px(180.)).flex_shrink_0()
                    })
                    .overflow_y_scroll()
                    .p(px(SPACE_MD))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(input)
                    .bg(table_bg)
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_SM));
                if let Some(row) = row {
                    let title = row["detailTitle"]
                        .as_str()
                        .or_else(|| row["name"].as_str())
                        .unwrap_or_default()
                        .to_owned();
                    let badge = row["detailBadge"].as_str().unwrap_or_default().to_owned();
                    let body_text = row["detail"].as_str().unwrap_or_default().to_owned();
                    let note_text = row["detailNote"].as_str().unwrap_or_default().to_owned();
                    let ok = row["detailOk"] != false;
                    if !title.is_empty() || !badge.is_empty() {
                        detail_card = detail_card.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(SPACE_SM))
                                .child(
                                    div()
                                        .min_w_0()
                                        .text_size(px(crate::theme::FONT_SECTION_TITLE))
                                        .line_height(px(crate::theme::LINE_SECTION_TITLE))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(crate::theme::strong(cx))
                                        .child(title),
                                )
                                .when(!badge.is_empty(), |header| {
                                    header.child(pill("detail-badge", None, Some(badge), cx))
                                }),
                        );
                    }
                    if !body_text.is_empty() {
                        detail_card = detail_card.child(
                            div()
                                .text_size(px(FONT_BODY))
                                .line_height(px(LINE_BODY))
                                .text_color(if ok { foreground } else { danger })
                                .child(body_text),
                        );
                    }
                    if !note_text.is_empty() {
                        detail_card = detail_card.child(
                            div()
                                .pt(px(SPACE_XS))
                                .border_t_1()
                                .border_color(border)
                                .text_size(px(FONT_CAPTION))
                                .line_height(px(LINE_CAPTION))
                                .text_color(crate::theme::quiet(cx))
                                .child(note_text),
                        );
                    }
                } else {
                    let hint = self.model["detail"].as_str().unwrap_or_default().to_owned();
                    detail_card = detail_card
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_size(px(FONT_BODY))
                        .line_height(px(LINE_BODY))
                        .text_color(muted_foreground)
                        .child(gpui_kit::component::Icon::empty().path("icons/info.svg").size_5())
                        .when(!hint.is_empty(), |card| card.child(hint));
                }
                if side_by_side {
                    let pane_h = super::dialog::fit_height(320., 220., 360., window);
                    stack.add(
                        div()
                            .w_full()
                            .h(px(pane_h))
                            .flex_shrink_0()
                            .flex()
                            .gap(px(SPACE_MD))
                            .child(table_box)
                            .child(div().flex_1().min_w_0().h_full().pt(px(TABLE_HEAD)).child(detail_card)),
                        SPACE_SM,
                    );
                } else {
                    stack.add(table_box, SPACE_SM);
                    stack.add(detail_card, SPACE_SM);
                }
            } else {
                stack.add(table_box, SPACE_SM);
            }
        }
        // What the list is about, under its buttons when the model says so (a scope warning).
        let note = (!guide.is_empty() && guide_below).then(|| super::notice::note(guide, cx));
        let mut note = note;
        if self.model["buttonsLast"] == true {
            if let Some(note) = note.take() {
                stack.add(note, SPACE_SM);
            }
        }
        if let (Some(table), Some(row)) = (&self.table, selected) {
            let table = table.read(cx);
            let rows = table.delegate();
            let mut details = div()
                .id("selection-details")
                .max_h(px(112.))
                .overflow_y_scroll()
                .flex()
                .flex_wrap()
                .gap(px(SPACE_SM))
                .px(px(SPACE_MD))
                .text_size(px(FONT_CAPTION))
                .line_height(px(LINE_CAPTION))
                .text_color(crate::theme::quiet(cx));
            let mut any = false;
            // Use the selected id because a search can change its position in the displayed model.
            let record = self.model["rows"].get(row).unwrap_or(&Value::Null);
            for column in rows.hidden_columns() {
                let cell = &record["cells"][column];
                let value = cell
                    .as_str()
                    .or_else(|| cell["text"].as_str())
                    .map(str::to_owned)
                    .or_else(|| {
                        cell["platforms"]
                            .as_array()
                            .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                    })
                    .unwrap_or_default();
                if value.is_empty() {
                    continue;
                }
                any = true;
                let label = self.model["columns"][column]["label"].as_str().unwrap_or_default();
                details = details.child(div().min_w_0().child(format!("{label}: {value}")));
            }
            if any {
                stack.add(details, SPACE_SM);
            }
        }
        if self.model["detail"].is_string() && !detail_side {
            let row = selected.and_then(|row| self.model["rows"].get(row));
            let (text, ok) = match row {
                Some(row) => (row["detail"].as_str().unwrap_or_default(), row["detailOk"] != false),
                None => (self.model["detail"].as_str().unwrap_or_default(), true),
            };
            // Nothing to say (no selection, or a plugin without a description): no empty box.
            if self.model["detailStatus"] == true {
                let (icon, color) = super::health::appearance(
                    row.map_or("off", |row| row["detailHealth"].as_str().unwrap_or("off")),
                    cx,
                );
                stack.add(
                    div()
                        .id("detail")
                        .flex()
                        .items_center()
                        .gap(px(SPACE_SM))
                        .px(px(SPACE_MD))
                        .min_h(px(CONTROL_HEIGHT))
                        .flex_shrink_0()
                        .text_color(if row.is_none() {
                            cx.theme().muted_foreground
                        } else {
                            color
                        })
                        .child(
                            gpui_kit::component::Icon::empty()
                                .path(format!("icons/{icon}.svg"))
                                .small(),
                        )
                        .child(div().min_w_0().child(text.to_owned())),
                    SPACE_SM,
                );
            } else if !text.is_empty() {
                let detail_max = self.model["detailHeight"].as_f64().unwrap_or(170.) as f32;
                let detail_mono = self.model["detailMono"] == true;
                stack.add(
                    div()
                        .id("detail")
                        .when(self.model["detailFill"] == true, |detail| detail.flex_1().min_h_0())
                        .when(self.model["detailFill"] != true, |detail| {
                            detail.min_h(px(36.)).max_h(px(detail_max)).flex_shrink_0()
                        })
                        .overflow_y_scroll()
                        .p(px(SPACE_SM))
                        .bg(cx.theme().table)
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().input)
                        .when(detail_mono, |detail| {
                            detail
                                .text_size(px(FONT_CAPTION))
                                .line_height(px(LINE_CAPTION))
                                .font_family(cx.theme().mono_font_family.clone())
                        })
                        .when(!detail_mono, |detail| {
                            detail.text_size(px(FONT_BODY)).line_height(px(LINE_BODY))
                        })
                        .text_color(if row.is_none() {
                            cx.theme().muted_foreground
                        } else if ok {
                            cx.theme().foreground
                        } else {
                            cx.theme().danger
                        })
                        .child(text.to_owned()),
                    SPACE_SM,
                );
            }
        }
        if self.model["buttonsLast"] != true {
            if let Some(buttons) = self.button_rows(window, cx) {
                // With a note under them, 8 between; the buttons are the last of the list otherwise.
                stack.add(buttons, SPACE_SM);
            }
            if let Some(note) = note.take() {
                stack.add(note, 0.);
            }
        }
        if self.model["connection"].is_object() && self.model["headerAdd"] != true {
            let room = self.available_width(window, cx);
            let (icon, color) =
                super::health::appearance(self.model["connection"]["health"].as_str().unwrap_or_default(), cx);
            stack.add(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_SM))
                    .h(px(CONTROL_HEIGHT))
                    .text_color(color)
                    .child(
                        gpui_kit::component::Icon::empty()
                            .path(format!("icons/{icon}.svg"))
                            .small(),
                    )
                    .child(self.model["connection"]["text"].as_str().unwrap_or_default().to_owned()),
                SPACE_SM,
            );
            let mut buttons = div().flex().gap(px(SPACE_SM)).w_full();
            // Like the other rows of buttons: labels fold to icons when they no longer fit.
            let wanted: f32 = Action::CONNECTIONS
                .iter()
                .map(|action| label_width(&action.row_label(&self.model, selected)) + 8.)
                .sum();
            let fold_connections = wanted > room;
            for action in Action::CONNECTIONS {
                buttons = buttons.child(
                    Button::new(SharedString::from(action.id(&self.model)))
                        .small()
                        .h(px(CONTROL_HEIGHT))
                        .flex_1()
                        .when(!fold_connections, |button| {
                            button.label(action.row_label(&self.model, selected))
                        })
                        .when(fold_connections, |button| {
                            button.tooltip(action.row_label(&self.model, selected))
                        })
                        .icon(action.icon(&self.model))
                        .disabled(!action.enabled(&self.model, selected))
                        .on_click(cx.listener(move |list, _, window, cx| list.action(action, None, window, cx))),
                );
            }
            stack.add(buttons, 0.);
        }
        // Under its own spacers: the stack sets the gaps, the body none.
        body.gap(px(0.)).children(stack.items).into_any_element()
    }
}

/// A read-only name and value table in a dialog.
pub(crate) fn show_values(values: &Value, window: &mut Window, cx: &mut App) {
    let title = values["title"].as_str().unwrap_or_default().to_owned();
    let hint = values["hint"].as_str().unwrap_or_default().to_owned();
    let rows: Vec<(String, String)> = values["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row["name"].as_str().unwrap_or_default().to_owned(),
                row["value"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    window.open_dialog(cx, move |dialog, window, cx| {
        let close = crate::controls::dialog::close_footer(cx);
        let max_h = crate::controls::dialog::fit_height(210., 140., 460., window);
        let key_w = (crate::controls::dialog::fit_width(640., window) * 0.3).clamp(120., 200.);
        crate::controls::dialog::shell(dialog, title.clone(), 640., window, cx)
            .footer(close)
            .child(crate::controls::dialog::body(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_SM))
                    .child(div().text_color(cx.theme().muted_foreground).child(hint.clone()))
                    .child(
                        div()
                            .id("values")
                            .max_h(px(max_h))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_XS))
                            .children(rows.iter().map(|(name, value)| {
                                div()
                                    .flex()
                                    .gap(px(SPACE_SM))
                                    .child(
                                        div()
                                            .w(px(key_w))
                                            .flex_shrink_0()
                                            .font_weight(FontWeight::BOLD)
                                            .child(name.clone()),
                                    )
                                    .child(div().flex_1().min_w_0().child(value.clone()))
                            })),
                    ),
            ))
    });
}

#[cfg(test)]
mod tests {
    use super::{table_height, Action, List, Stack};
    use crate::theme::{SPACE_MD, SPACE_SM};
    use gpui_kit::div;
    use serde_json::json;

    #[test]
    fn filter_toggle_supports_platform_exclusion_and_independent_toggles() {
        let platforms = json!({
            "options": [
                {"id": "twitch", "shown": true},
                {"id": "kick", "shown": true}
            ]
        });
        assert_eq!(List::next_filter_hidden(&platforms, 0), Some(vec!["twitch".into()]));
        let one_left = json!({
            "options": [
                {"id": "twitch", "shown": false},
                {"id": "kick", "shown": true}
            ]
        });
        assert_eq!(
            List::next_filter_hidden(&one_left, 1),
            None,
            "cannot hide last platform"
        );
        assert_eq!(List::next_filter_hidden(&one_left, 0), Some(vec![]));

        let toggles = json!({
            "mode": "toggles",
            "options": [
                {"id": "commercial", "shown": false},
                {"id": "my_language", "shown": true}
            ]
        });
        assert_eq!(
            List::next_filter_hidden(&toggles, 0),
            Some(vec!["my_language".into(), "commercial".into()])
        );
        assert_eq!(
            List::next_filter_hidden(&toggles, 1),
            Some(vec![]),
            "toggles mode allows turning off all filters"
        );
    }

    #[test]
    fn progress_percent_clamps_bounds_and_ignores_missing() {
        assert_eq!(List::progress_percent(&json!(null)), None);
        assert_eq!(List::progress_percent(&json!({"percent": -5})), Some(0.0));
        assert_eq!(List::progress_percent(&json!({"percent": 42})), Some(42.0));
        assert_eq!(List::progress_percent(&json!({"percent": 150})), Some(100.0));
    }

    #[test]
    fn selected_row_record_applies_per_row_confirm_for_toolbar_buttons() {
        let model = json!({
            "rows": [
                {"id": "a", "confirm": {"download": "Download A (CC0)"}},
                {"id": "b", "confirm": {"download": "Download B (Non-commercial)"}}
            ]
        });
        let from_menu = List::row_record(&model, Some(0), "b").unwrap();
        assert_eq!(from_menu["confirm"]["download"], "Download A (CC0)");
        let from_button = List::row_record(&model, None, "b").unwrap();
        assert_eq!(from_button["confirm"]["download"], "Download B (Non-commercial)");
        assert!(List::row_record(&model, None, "").is_none());
        // An out-of-bounds explicit row must not fall back to a different selected row.
        assert!(List::row_record(&model, Some(9), "b").is_none());
        assert!(List::row_record(&model, Some(usize::MAX), "b").is_none());
    }

    #[test]
    fn custom_action_guard_rejects_out_of_bounds_and_per_row_confirm_works_without_spec_confirm() {
        let model = json!({
            "actions": [
                {"id": "remove", "label": "Remove", "needs": "row"}
            ],
            "rows": [
                {"id": "a", "confirm": {"remove": "Remove item A?"}}
            ]
        });
        assert!(List::action_allowed(&model, Action::Custom(0)));
        assert!(!List::action_allowed(&model, Action::Custom(1)));
        assert!(!List::action_allowed(&model, Action::Custom(99)));

        let prompt = List::confirm_prompt(&model["actions"][0], Some(&model["rows"][0]));
        assert_eq!(prompt, Some(("Remove item A?".to_owned(), "Remove".to_owned())));
        assert_eq!(List::confirm_prompt(&model["actions"][0], None), None);
    }

    #[test]
    fn f2_prefers_explicit_rename_over_edit_and_respects_row_permissions() {
        let overlay = json!({
            "actions": [
                {"id": "copy_url", "needs": "row"},
                {"id": "rename", "needs": "row"},
                {"id": "settings", "needs": "row"}
            ],
            "rows": [
                {"id": "Clock:index.html", "can": {}},
                {"id": "Locked:index.html", "can": {"rename": false}}
            ]
        });
        assert!(matches!(List::f2_action(&overlay, 0), Some(Action::Custom(1))));
        assert!(List::f2_action(&overlay, 1).is_none());

        let queues = json!({
            "actions": ["add", "edit", "delete"],
            "rows": [
                {"id": "default", "can": {"edit": false}},
                {"id": "custom", "can": {"edit": true}}
            ]
        });
        assert!(List::f2_action(&queues, 0).is_none());
        assert!(matches!(List::f2_action(&queues, 1), Some(Action::Edit)));

        let history = json!({
            "actions": ["values_before", "values_after", "clear"],
            "rows": [{"id": "h1"}]
        });
        assert!(List::f2_action(&history, 0).is_none());
    }

    #[test]
    fn a_table_sized_to_its_rows_keeps_a_few_and_stops_at_its_most() {
        // Header 24, rows of 28, one line for the box's bottom edge.
        assert_eq!(table_height(0, 8), 24. + 3. * 28. + 1.);
        assert_eq!(table_height(1, 8), 24. + 3. * 28. + 1.);
        assert_eq!(table_height(5, 8), 24. + 5. * 28. + 1.);
        assert_eq!(table_height(40, 8), 24. + 8. * 28. + 1.);
        assert_eq!(table_height(9, 2), table_height(3, 3));
    }

    #[test]
    fn the_gap_under_a_block_is_drawn_only_when_another_block_follows() {
        let mut stack = Stack::new();
        stack.add(div(), SPACE_MD);
        assert_eq!(stack.items.len(), 1);
        stack.add(div(), SPACE_SM);
        assert_eq!(stack.items.len(), 3, "block, 12px gap, block");
        stack.add(div(), 0.);
        assert_eq!(stack.items.len(), 5, "an 8px gap, then the block");
        stack.add(div(), SPACE_SM);
        assert_eq!(stack.items.len(), 6, "no gap spacer under a block that wants none");
        let column = stack.column();
        let _ = column;
        assert!(stack.items.is_empty());
    }

    #[cfg(feature = "test-support")]
    mod ui {
        use super::super::List;
        use gpui_kit::test::{TestQueryExt, TestWindowExt};
        use gpui_kit::{
            div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Role, Styled,
            TestAppContext, Window, WindowBounds, WindowOptions,
        };
        use serde_json::json;

        struct ListHost {
            list: Entity<List>,
            width: f32,
        }

        impl Render for ListHost {
            fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
                div().w(px(self.width)).h(px(600.)).child(self.list.clone())
            }
        }

        fn commands_model() -> serde_json::Value {
            json!({
                "kind": "command",
                "title": "Command",
                "columns": [
                    {"key": "enabled", "label": "On", "width": 48},
                    {"key": "name", "label": "Name", "width": 138},
                    {"key": "commands", "label": "Commands", "width": 138},
                    {"key": "who", "label": "Who", "width": 122},
                    {"key": "sources", "label": "Sources", "width": 112},
                    {"key": "cooldown", "label": "Cooldown", "width": 114},
                    {"key": "group", "label": "Group", "width": 106}
                ],
                "rows": [
                    {
                        "id": "cmd-1",
                        "cells": [
                            true,
                            "Manage commands",
                            "!commands",
                            {"icon": "users", "text": "Everyone"},
                            {"platforms": ["twitch", "youtube", "kick", "tiktok"]},
                            {"icon": "timer", "text": "10 s each"},
                            {"icon": "folder", "text": "Built-in"}
                        ]
                    }
                ]
            })
        }

        fn open_list(
            width: f32,
            initial_fit: Option<f32>,
            cx: &mut TestAppContext,
        ) -> (
            gpui_kit::WindowHandle<gpui_kit::base::Root>,
            Entity<ListHost>,
            Entity<List>,
        ) {
            cx.update(|cx| {
                gpui_kit::init(cx);
                crate::theme::apply(true, cx);
            });
            let list_slot: std::rc::Rc<std::cell::RefCell<Option<Entity<List>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let host_slot: std::rc::Rc<std::cell::RefCell<Option<Entity<ListHost>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let l_slot = list_slot.clone();
            let h_slot = host_slot.clone();
            let window = cx.update(|cx| {
                let bounds = Bounds {
                    origin: gpui_kit::Point::default(),
                    size: size(px(1200.), px(700.)),
                };
                let (window, _) = gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    cx,
                    move |window, cx| {
                        let list = cx.new(|cx| {
                            let mut list = List::new(commands_model(), "test:list".into(), window, cx);
                            if let Some(fit) = initial_fit {
                                list.set_fit(fit, cx);
                            }
                            list
                        });
                        *l_slot.borrow_mut() = Some(list.clone());
                        let host = cx.new(|_| ListHost {
                            list: list.clone(),
                            width,
                        });
                        *h_slot.borrow_mut() = Some(host.clone());
                        host
                    },
                )
                .expect("open test window");
                window.downcast::<gpui_kit::base::Root>().expect("root window")
            });
            let host = host_slot.borrow().clone().expect("host created");
            let list = list_slot.borrow().clone().expect("list created");
            (window, host, list)
        }

        fn header_span(window: &Window) -> f32 {
            let mut headers = window.find_all_by_role(Role::ColumnHeader);
            headers.sort_by(|a, b| a.bounds().origin.x.partial_cmp(&b.bounds().origin.x).unwrap());
            let first = headers.first().expect("column headers");
            let last = headers.last().expect("column headers");
            f32::from(last.bounds().right() - first.bounds().left())
        }

        #[gpui_kit::test]
        fn table_fills_initial_width_on_first_frame_without_sitting_at_min_width(cx: &mut TestAppContext) {
            let (window, _host, list) = open_list(1000., Some(1000.), cx);
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let fit = list.read(cx).table.as_ref().expect("table").read(cx).delegate().fit;
                assert!((fit - 1000.).abs() <= 1., "expected fit=1000 on frame 1, got {fit}");
                let span = header_span(window);
                assert!(
                    (span - 1000.).abs() <= 2.,
                    "first frame column span should fill 1000px container, got {span}"
                );
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn table_expands_after_container_grows_without_table_interaction(cx: &mut TestAppContext) {
            let (window, host, list) = open_list(500., Some(500.), cx);
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let narrow_span = header_span(window);
                assert!(
                    (narrow_span - 500.).abs() <= 2.,
                    "expected narrow span ~500, got {narrow_span}"
                );

                host.update(cx, |host, cx| {
                    host.width = 1000.;
                    cx.notify();
                });
                window.render_frame(cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let fit = list.read(cx).table.as_ref().expect("table").read(cx).delegate().fit;
                assert!((fit - 1000.).abs() <= 1., "expected resized fit=1000, got {fit}");
                let wide_span = header_span(window);
                assert!(
                    (wide_span - 1000.).abs() <= 2.,
                    "resized column span should fill 1000px container, got {wide_span}"
                );
            })
            .unwrap();
        }

        #[gpui_kit::test]
        fn search_filters_rows_by_text_platforms_brand_and_preserves_or_clears_selection(cx: &mut TestAppContext) {
            cx.update(|cx| {
                gpui_kit::init(cx);
                crate::theme::apply(true, cx);
            });
            let model = json!({
                "kind": "command",
                "title": "Commands",
                "search": true,
                "searchHint": "Search commands",
                "empty": "No commands yet.",
                "columns": [
                    {"key": "name", "label": "Name", "width": 200},
                    {"key": "sources", "label": "Sources", "width": 160}
                ],
                "rows": [
                    {
                        "id": "cmd-1",
                        "cells": [
                            "Manage commands",
                            {"platforms": ["twitch", "youtube"]}
                        ]
                    },
                    {
                        "id": "cmd-2",
                        "cells": [
                            {"icon": "film", "text": "Switch Scene"},
                            {"brand": "obs"}
                        ]
                    }
                ]
            });
            let list_slot: std::rc::Rc<std::cell::RefCell<Option<Entity<List>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));
            let l_slot = list_slot.clone();
            let window = cx.update(|cx| {
                let bounds = Bounds {
                    origin: gpui_kit::Point::default(),
                    size: size(px(900.), px(600.)),
                };
                let (window, _) = gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    cx,
                    move |window, cx| {
                        let list = cx.new(|cx| {
                            let mut list = List::new(model, "test:search".into(), window, cx);
                            list.set_fit(800., cx);
                            list
                        });
                        *l_slot.borrow_mut() = Some(list.clone());
                        cx.new(|_| ListHost { list, width: 800. })
                    },
                )
                .expect("open test window");
                window.downcast::<gpui_kit::base::Root>().expect("root window")
            });
            let list = list_slot.borrow().clone().expect("list created");

            let type_query = |text: &str, list: &Entity<List>, window: &mut Window, cx: &mut gpui_kit::App| {
                use gpui_kit::test::TestEventExt;
                let search = list.read(cx).search.clone().expect("search input");
                search.update(cx, |input, cx| input.focus(window, cx));
                window.key_down("ctrl-a", false, cx);
                window.key_up("ctrl-a", cx);
                if text.is_empty() {
                    window.key_down("backspace", false, cx);
                    window.key_up("backspace", cx);
                } else {
                    window.input(text, cx);
                }
            };

            let shown_ids = |list: &Entity<List>, cx: &gpui_kit::App| -> Vec<String> {
                list.read(cx).table.as_ref().expect("table").read(cx).delegate().model["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|row| row["id"].as_str().map(str::to_owned))
                    .collect()
            };

            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(shown_ids(&list, cx), vec!["cmd-1", "cmd-2"]);
                // Select cmd-2 ("Switch Scene"), then search by text to ensure selection stays.
                let table = list.read(cx).table.clone().expect("table");
                table.update(cx, |table, cx| table.set_selected_row(1, cx));
                type_query("Scene", &list, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(shown_ids(&list, cx), vec!["cmd-2"]);
                assert_eq!(list.read(cx).selected_id, "cmd-2");

                // Search by platform matches cmd-1 and clears hidden cmd-2 selection.
                type_query("twitch", &list, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(shown_ids(&list, cx), vec!["cmd-1"]);
                assert!(list.read(cx).selected_id.is_empty());

                // Search by brand matches cmd-2.
                type_query("OBS", &list, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(shown_ids(&list, cx), vec!["cmd-2"]);

                // Non-matching search keeps table visible (truly_empty is false) with "No matches" text.
                type_query("zzz-nomatch", &list, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(shown_ids(&list, cx).is_empty());
                assert!(!list.read(cx).truly_empty(cx));
                let empty_text = list.read(cx).table.as_ref().expect("table").read(cx).delegate().model["empty"]
                    .as_str()
                    .map(str::to_owned);
                assert_eq!(empty_text.as_deref(), Some(crate::i18n::tr("lists.no_matches")));

                // Clearing search restores both rows.
                type_query("", &list, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(shown_ids(&list, cx), vec!["cmd-1", "cmd-2"]);

                // Shrinking the list while the last row is selected clears selection without panicking.
                let table = list.read(cx).table.clone().expect("table");
                table.update(cx, |table, cx| table.set_selected_row(1, cx));
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                assert_eq!(list.read(cx).selected_id, "cmd-2");
                let shrunk = json!({
                    "kind": "command",
                    "title": "Commands",
                    "search": true,
                    "columns": [
                        {"key": "name", "label": "Name", "width": 200},
                        {"key": "sources", "label": "Sources", "width": 160}
                    ],
                    "rows": [
                        {"id": "", "cells": ["Blank ID", {"platforms": []}]},
                        {"id": "cmd-1", "cells": ["Manage commands", {"platforms": ["twitch"]}]}
                    ]
                });
                list.update(cx, |list, cx| list.update_model(shrunk, window, cx));
                window.render_frame(cx);
                // Selection cleared and does not phantom-select the empty-id row.
                assert!(list.read(cx).selected_id.is_empty());
                assert_eq!(list.read(cx).table.as_ref().unwrap().read(cx).selected_row(), None);

                // Shrinking to zero rows also renders cleanly without panicking.
                let empty = json!({
                    "kind": "command",
                    "title": "Commands",
                    "search": true,
                    "empty": "No commands yet.",
                    "columns": [{"key": "name", "label": "Name", "width": 200}],
                    "rows": []
                });
                list.update(cx, |list, cx| list.update_model(empty, window, cx));
                window.render_frame(cx);
                assert!(list.read(cx).selected_id.is_empty());
            })
            .unwrap();
        }
    }
}
