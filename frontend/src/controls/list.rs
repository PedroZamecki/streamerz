//! One controller for every editable entity list, including selection, filtering and dialogs.
use super::{
    card::Card,
    form::Form,
    rows::{Action, Rows},
    sections::Sections,
};
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
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
}
impl EventEmitter<ViewChange> for List {}

pub struct List {
    model: Value,
    table: Option<Entity<TableState<Rows>>>,
    search: Option<Entity<InputState>>,
    /// The page's tabs (label, options, selected): drawn as a "Show" combo before the search.
    show: Option<Value>,
    lead_outside: bool,
    selected_id: String,
    form: Option<Entity<Form>>,
    form_subscription: Option<Subscription>,
    error: String,
    route: String,
    /// A list in a dialog (a group's members): its entity and the request that fills it.
    dialog_list: Option<Entity<List>>,
    dialog_request: Value,
    /// A person's card open in a dialog.
    card: Option<Entity<Card>>,
    /// Fields drawn in place of a settings row (the MCP connection's address, port and route).
    inline: Option<Entity<Sections>>,
    rows_view: Entity<ButtonRows>,
    _subscriptions: Vec<Subscription>,
}
/// About how wide a button with `label` is (icon, padding and text at the small size).
fn label_width(label: &str) -> f32 {
    52. + 7. * label.chars().count() as f32
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
        let table = (model["presentation"] != "settings").then(|| {
            cx.new(|cx| {
                TableState::new(
                    Rows {
                        model: model.clone(),
                        action,
                        sort: sorter,
                        fit: 0.,
                    },
                    window,
                    cx,
                )
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
        let mut this = Self {
            model,
            table,
            search,
            show: None,
            lead_outside: false,
            selected_id: String::new(),
            form: None,
            form_subscription: None,
            error: String::new(),
            route,
            dialog_list: None,
            dialog_request: Value::Null,
            card: None,
            inline,
            rows_view,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        this
    }
    /// Shows or hides the `index`th option of the model's filter; one is always left showing.
    fn toggle_filter(&mut self, index: usize, cx: &mut Context<Self>) {
        let options = self.model["filter"]["options"].as_array().cloned().unwrap_or_default();
        let mut hidden: Vec<String> = options
            .iter()
            .filter(|option| option["shown"] == false)
            .filter_map(|option| option["id"].as_str().map(str::to_owned))
            .collect();
        let Some(id) = options.get(index).and_then(|option| option["id"].as_str()) else {
            return;
        };
        if let Some(position) = hidden.iter().position(|hidden| hidden == id) {
            hidden.remove(position);
        } else if hidden.len() + 1 < options.len() {
            hidden.push(id.to_owned());
        } else {
            return;
        }
        let kind = self.kind().to_owned();
        cx.emit(ViewChange::Hidden { kind, hidden });
    }
    pub fn is_settings(&self) -> bool {
        self.model["presentation"] == "settings"
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
    pub fn has_actions(&self) -> bool {
        !Action::base_actions(&self.model).is_empty()
    }
    pub fn has_inline(&self) -> bool {
        self.model["inline"].is_object()
    }
    /// A page's introduction and its state line (above the first table).
    pub fn intro_block(&self, cx: &App) -> Option<AnyElement> {
        if self.model["intro"].is_null() && !self.model["status"].is_object() {
            return None;
        }
        let mut block = div().flex().flex_col().gap(px(8.)).mb(px(-5.));
        if let Some(intro) = self.model["intro"].as_str() {
            block = block.child(
                div()
                    .min_h(px(36.))
                    .flex()
                    .items_start()
                    .pt(px(5.))
                    .px(px(3.))
                    .text_color(crate::theme::quiet(cx))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(13.4))
                            .line_height(px(17.))
                            .child(intro.to_owned()),
                    ),
            );
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
                    .gap_2()
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
    pub fn set_show(&mut self, tabs: Option<Value>, cx: &mut Context<Self>) {
        if self.show != tabs {
            self.show = tabs;
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
        }
        self.model = model;
        self.refresh(cx);
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
                    })
                });
            }
            model["empty"] = tr("lists.no_matches").into();
        }
        let mut selection = model["rows"]
            .as_array()
            .and_then(|rows| rows.iter().position(|row| row["id"] == self.selected_id));
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
                table.refresh(cx);
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
    fn action(&mut self, action: Action, row: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let shown = match &self.table {
            Some(table) => table.read(cx).delegate().model.clone(),
            None => self.model.clone(),
        };
        let record = row.and_then(|row| shown["rows"].get(row).cloned());
        let kind = record
            .as_ref()
            .and_then(|row| row["kind"].as_str())
            .unwrap_or_else(|| self.kind())
            .to_owned();
        let id = record
            .as_ref()
            .and_then(|row| row["id"].as_str())
            .unwrap_or(&self.selected_id)
            .to_owned();
        let selected = shown["rows"]
            .as_array()
            .and_then(|rows| rows.iter().position(|row| !id.is_empty() && row["id"] == id));
        if !Action::base_actions(&shown)
            .iter()
            .any(|listed| std::mem::discriminant(listed) == std::mem::discriminant(&action))
            && !matches!(action, Action::Connect | Action::ConnectAll | Action::DisconnectAll)
        {
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
                match spec["confirm"].as_str() {
                    None => send(request),
                    Some(question) => {
                        let question = question.to_owned();
                        let label = spec["confirmLabel"].as_str().unwrap_or_default().to_owned();
                        let danger = spec["danger"] == true;
                        window.open_dialog(cx, move |dialog, _, cx| {
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
                                .gap_2()
                                .child(
                                    Button::new("cancel-action")
                                        .label(tr("dialogs.cancel"))
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                )
                                .child(if danger { confirm.danger() } else { confirm });
                            let footer = crate::controls::dialog::footer(None, buttons, cx);
                            crate::controls::dialog::shell(dialog, label.clone(), 448., cx)
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
                    Action::Connect if selected.is_some_and(|row| shown["rows"][row]["connected"] == true) => {
                        "disconnect"
                    }
                    _ => "connect",
                };
                send(
                    json!({"request": "StreamerzConnection", "kind": kind, "itemId": if matches!(action, Action::Connect) { &id } else { "" }, "action": operation, "id": format!("{route}:connection")}),
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
                window.open_dialog(cx, move |dialog, _, cx| {
                    let kind = kind.clone(); let id = id.clone(); let route = route.clone();
                    let buttons = div().id("delete-confirmation").test_support().role(Role::Group).aria_label(confirm_label.clone()).flex().justify_end().gap_2()
                        .child(Button::new("cancel-delete").label(tr("dialogs.cancel")).on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("confirm-delete").danger().label(tr("btn.delete")).accessibility_label(confirm_label.clone()).on_click(move |_, window, cx| { send(json!({"request": "StreamerzDelete", "what": kind, "itemId": id, "id": format!("{route}:delete")})); window.close_dialog(cx); }));
                    let footer = crate::controls::dialog::footer(None, buttons, cx);
                    crate::controls::dialog::shell(dialog, tr("btn.delete"), 448., cx)
                        .child(crate::controls::dialog::body(question.clone()))
                        .footer(footer)
                });
            }
        }
    }
    /// Returns whether the page should query the core again.
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if value["status"] == "error" {
            let error = value["error"].as_str().unwrap_or_default().to_owned();
            if matches!(operation, "save" | "json") {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.pending = false;
                        form.error = error;
                        cx.notify();
                    });
                }
            } else {
                self.error = error;
            }
            cx.notify();
            return false;
        }
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
                window.open_dialog(cx, move |dialog, _, _| {
                    let owner = owner.clone();
                    dialog
                        .centered()
                        .pt(px(12.))
                        .close_button(false)
                        .pt(px(0.))
                        .pb(px(0.))
                        .pl(px(0.))
                        .pr(px(0.))
                        .width(px(520.))
                        .child(card.clone())
                        .on_cancel(move |_, _, cx| {
                            let _ = owner.update(cx, |list, _| list.card = None);
                            true
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
                let model = value["tables"][0].clone();
                let title = model["title"].as_str().unwrap_or_default().to_owned();
                let nested = cx.new(|cx| List::new(model, format!("{}:dlist", self.route), window, cx));
                self.dialog_list = Some(nested.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, _, cx| {
                    let owner = owner.clone();
                    crate::controls::dialog::shell(dialog, title.clone(), 560., cx)
                        .child(crate::controls::dialog::body(div().h(px(360.)).child(nested.clone())))
                        .footer(crate::controls::dialog::close_footer(cx))
                        .on_cancel(move |_, _, cx| {
                            let _ = owner.update(cx, |list, _| list.dialog_list = None);
                            true
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
                if let Some(nested) = &self.dialog_list {
                    nested.update(cx, |list, cx| list.update_model(value["tables"][0].clone(), window, cx));
                }
                false
            }
            "form" => {
                let title = value["title"].as_str().unwrap_or_default().to_owned();
                let route = format!("{}:save", self.route);
                let form = cx.new(|cx| Form::new(value, route, window, cx));
                self.form_subscription = Some(cx.subscribe(&form, |list, _, _: &super::form::Cancelled, cx| {
                    list.form = None;
                    list.form_subscription = None;
                    cx.notify();
                }));
                self.form = Some(form.clone());
                let owner = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, _, cx| {
                    crate::controls::dialog::shell(dialog, title.clone(), 560., cx)
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
                                    list.form_subscription = None;
                                    cx.notify();
                                });
                                true
                            }
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
                self.form_subscription = None;
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
    /// The grid of buttons under the table (a page may draw it later, after another section).
    pub fn button_rows(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let selected = self.model["rows"].as_array().and_then(|rows| {
            rows.iter()
                .position(|row| !self.selected_id.is_empty() && row["id"] == self.selected_id)
        });
        let actions = Action::base_actions(&self.model);
        let per_row = self.model["perRow"].as_u64().unwrap_or(4).max(1) as usize;
        let mut buttons = div().flex().flex_col().gap(px(6.)).w_full().mt(px(2.));
        // Buttons fold to their icons when a label no longer fits (docs/UI.md): all of them together.
        let room = self.table.as_ref().map_or(0., |table| table.read(cx).delegate().fit);
        let room = if room > 0. {
            room
        } else {
            f32::from(window.viewport_size().width) - 200.
        };
        let fold = actions.chunks(per_row).any(|chunk| {
            let wanted: f32 = chunk.iter().map(|action| label_width(&action.label(&self.model))).sum();
            wanted + 8. * chunk.len() as f32 > room
        });
        // Folded to icons, they all fit on one line.
        let per_row = if fold { actions.len().max(1) } else { per_row };
        for chunk in actions.chunks(per_row) {
            let mut line = div().flex().gap_2().w_full();
            for action in chunk.iter().copied() {
                let id = match action {
                    Action::Custom(index) => SharedString::from(format!("custom-{index}")),
                    _ => SharedString::from(action.id(&self.model)),
                };
                line = line.child(
                    Button::new(id)
                        .small()
                        .h(px(28.))
                        .flex_1()
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
                );
            }
            // A short last row keeps the width of the others (1.6.0 lays buttons out on a grid).
            for _ in chunk.len()..per_row.min(actions.len()) {
                line = line.child(div().flex_1());
            }
            buttons = buttons.child(line);
        }
        // A list whose rows open on a click and has only that button (Details) draws none (1.6.0).
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
impl Render for List {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Columns follow the width the table really has (known after its first layout).
        if let Some(table) = &self.table {
            let seen = f32::from(table.read(cx).horizontal_scroll_handle.bounds().size.width);
            if seen > 1. && (seen - table.read(cx).delegate().fit).abs() > 1. {
                table.update(cx, |table, cx| {
                    table.delegate_mut().fit = seen;
                    table.refresh(cx);
                });
            }
        }
        let mut body = div().size_full().flex().flex_col().gap(px(2.)).pt(px(2.));
        let guide = self.model["guide"].as_str().unwrap_or_default();
        // A page's introduction and its state line, above the first table.
        if !self.lead_outside {
            if let Some(lead) = self.intro_block(cx) {
                body = body.child(lead);
            }
        }
        let titled = !self.model["title"].as_str().unwrap_or_default().is_empty();
        if titled && ((!guide.is_empty() && self.model["titleless"] != true) || self.model["showTitle"] == true) {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(24.))
                    .pl(px(2.))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(15.))
                    .text_color(crate::theme::strong(cx))
                    .when_some(self.model["brand"].as_str(), |title, brand| {
                        title.child(img(format!("brands/{brand}.svg")).id("title-brand").size_4())
                    })
                    .when_some(self.model["icon"].as_str(), |title, icon| {
                        title.child(
                            gpui_kit::component::Icon::empty()
                                .path(format!("icons/{icon}.svg"))
                                .small(),
                        )
                    })
                    .child(self.model["title"].as_str().unwrap_or_default().to_owned()),
            );
        }
        let guide_below = self.model["guideBelow"] == true;
        if !guide.is_empty() && !guide_below {
            body = body.child(
                div()
                    .min_h(px(36.))
                    .flex()
                    .items_center()
                    .px(px(3.))
                    .text_color(crate::theme::quiet(cx))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(13.4))
                            .line_height(px(19.))
                            .child(guide.to_owned()),
                    ),
            );
        }
        if !self.error.is_empty() {
            body = body.child(super::notice::error(&self.error, cx));
        }
        if let (true, Some(inline)) = (self.is_settings(), self.inline.clone()) {
            body = body.child(inline);
        } else if self.is_settings() {
            for (index, row) in self.model["rows"].as_array().into_iter().flatten().enumerate() {
                let (icon, color) = super::health::appearance(row["health"].as_str().unwrap_or_default(), cx);
                let label = row["text"].as_str().unwrap_or_default().to_owned();
                let name = row["name"].as_str().unwrap_or_default().to_owned();
                let owner = cx.entity().downgrade();
                body = body.child(
                    div()
                        .id(("setting-row", index))
                        .test_support()
                        .role(Role::Group)
                        .aria_label(name.clone())
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(26.))
                        .child(
                            gpui_kit::component::Icon::empty()
                                .path(format!("icons/{icon}.svg"))
                                .small()
                                .text_color(color),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(13.5))
                                .text_color(color)
                                .child(label),
                        )
                        .child(
                            Button::new(("edit-setting", index))
                                .small()
                                .w(px(90.))
                                .h(px(26.))
                                .icon(Action::Edit.icon(&Value::Null))
                                .label(tr("btn.edit"))
                                .accessibility_label(format!("{}: {name}", tr("btn.edit")))
                                .on_click(cx.listener(move |list, _, window, cx| {
                                    list.action(Action::Edit, Some(index), window, cx)
                                })),
                        )
                        .context_menu(move |menu, _, _| {
                            let owner = owner.clone();
                            menu.item(
                                PopupMenuItem::new(tr("btn.edit"))
                                    .icon(Action::Edit.icon(&Value::Null))
                                    .on_click(move |_, window, cx| {
                                        let _ = owner
                                            .update(cx, |list, cx| list.action(Action::Edit, Some(index), window, cx));
                                    }),
                            )
                        }),
                );
            }
            return body.into_any_element();
        }
        let options = self.model["filter"]["options"].as_array().cloned().unwrap_or_default();
        if self.search.is_some() || !options.is_empty() || self.show.is_some() {
            let mut line = div().flex().items_center().gap_2().w_full();
            if let Some(tabs) = self.show.clone() {
                let owner = cx.entity().downgrade();
                let current = tabs["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|option| option["id"] == tabs["selected"])
                    .and_then(|option| option["label"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                line = line
                    .child(div().child(tabs["label"].as_str().unwrap_or_default().to_owned()))
                    .child(
                        Button::new("show-combo")
                            .small()
                            .h(px(26.))
                            .w(px(108.))
                            .text_size(px(13.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .child(current)
                                    .child(Icon::empty().path("icons/chevrons-up-down.svg").size_3()),
                            )
                            .dropdown_menu(move |mut menu, _, _| {
                                for option in tabs["options"].as_array().into_iter().flatten() {
                                    let id = option["id"].as_str().unwrap_or_default().to_owned();
                                    let owner = owner.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(option["label"].as_str().unwrap_or_default().to_owned())
                                            .checked(option["id"] == tabs["selected"])
                                            .on_click(move |_, _, cx| {
                                                let _ = owner.update(cx, |_, cx| cx.emit(ViewChange::Tab(id.clone())));
                                            }),
                                    );
                                }
                                menu
                            }),
                    );
            }
            if let Some(search) = &self.search {
                line = line.child(
                    div().flex_1().min_w_0().h(px(26.)).child(
                        Input::new(search)
                            .small()
                            .min_h(px(26.))
                            .bg(cx.theme().table)
                            .text_size(px(13.))
                            .aria_label(
                                self.model["searchHint"]
                                    .as_str()
                                    .map_or_else(|| tr("lists.search").to_owned(), str::to_owned),
                            ),
                    ),
                );
            }
            // The platform column's filter: each platform in the list can be hidden, one stays.
            for (index, option) in options.iter().enumerate() {
                let label = option["label"].as_str().unwrap_or_default().to_owned();
                let shown = option["shown"] == true;
                line = line.child(
                    Button::new(("filter", index))
                        .small()
                        .when(shown, |button| button.outline())
                        .when(!shown, |button| button.ghost().opacity(0.4))
                        .tooltip(label.clone())
                        .accessibility_label(label.clone())
                        .child(
                            img(format!("brands/{}.svg", option["brand"].as_str().unwrap_or_default()))
                                .id(("filter-logo", index))
                                .size_4(),
                        )
                        .on_click(cx.listener(move |list, _, _, cx| list.toggle_filter(index, cx))),
                );
            }
            body = body.child(line);
        }
        if let Some(table) = &self.table {
            body = body.child(
                // 1.6.0: the header is a quiet line above a rounded box (drawn over the rows, no fill).
                div()
                    .relative()
                    .when_some(self.model["tableHeight"].as_f64(), |wrap, height| {
                        wrap.h(px(height as f32)).flex_shrink_0()
                    })
                    .when(self.model["tableHeight"].is_null(), |wrap| wrap.flex_1().min_h_0())
                    .child(
                        DataTable::new(table)
                            .accessibility_label(self.model["title"].as_str().unwrap_or_default().to_owned())
                            .bordered(false)
                            .header_row_height(px(23.))
                            .with_size(gpui_kit::component::Size::Size(px(28.))),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(22.))
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .border_1()
                            .border_color(cx.theme().input)
                            .rounded(px(6.)),
                    ),
            );
        }
        if !guide.is_empty() && guide_below {
            body = body.child(
                div()
                    .min_h(px(36.))
                    .flex()
                    .items_center()
                    .px(px(3.))
                    .text_color(crate::theme::quiet(cx))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(13.5))
                            .line_height(px(19.))
                            .child(guide.to_owned()),
                    ),
            );
        }
        let selected = self.model["rows"].as_array().and_then(|rows| {
            rows.iter()
                .position(|row| !self.selected_id.is_empty() && row["id"] == self.selected_id)
        });
        if self.model["detail"].is_string() {
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
                body = body.child(
                    div()
                        .id("detail")
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .min_h(px(28.))
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
                );
            } else if !text.is_empty() {
                body = body.child(
                    div()
                        .id("detail")
                        .when(self.model["detailFill"] == true, |detail| detail.flex_1().min_h_0())
                        .when(self.model["detailFill"] != true, |detail| {
                            detail
                                .h(px(self.model["detailHeight"].as_f64().unwrap_or(170.) as f32))
                                .flex_shrink_0()
                        })
                        .overflow_y_scroll()
                        .p(px(4.))
                        .bg(cx.theme().table)
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().input)
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(if row.is_none() {
                            cx.theme().muted_foreground
                        } else if ok {
                            cx.theme().foreground
                        } else {
                            cx.theme().danger
                        })
                        .child(text.to_owned()),
                );
            }
        }
        if self.model["buttonsLast"] != true {
            if let Some(buttons) = self.button_rows(window, cx) {
                body = body.child(buttons);
            }
        }
        if self.model["connection"].is_object() {
            let room = self.table.as_ref().map_or(0., |table| table.read(cx).delegate().fit);
            let room = if room > 0. {
                room
            } else {
                f32::from(window.viewport_size().width) - 200.
            };
            let (icon, color) =
                super::health::appearance(self.model["connection"]["health"].as_str().unwrap_or_default(), cx);
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(26.))
                    .text_color(color)
                    .child(
                        gpui_kit::component::Icon::empty()
                            .path(format!("icons/{icon}.svg"))
                            .small(),
                    )
                    .child(self.model["connection"]["text"].as_str().unwrap_or_default().to_owned()),
            );
            let mut buttons = div().flex().gap_2().w_full();
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
                        .h(px(28.))
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
            body = body.child(buttons);
        }
        body.into_any_element()
    }
}

/// A read-only name and value table in a dialog.
fn show_values(values: &Value, window: &mut Window, cx: &mut App) {
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
    window.open_dialog(cx, move |dialog, _, cx| {
        let close = crate::controls::dialog::close_footer(cx);
        crate::controls::dialog::shell(dialog, title.clone(), 640., cx)
            .footer(close)
            .child(crate::controls::dialog::body(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_color(cx.theme().muted_foreground).child(hint.clone()))
                    .child(
                        div()
                            .id("values")
                            .max_h(px(360.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(rows.iter().map(|(name, value)| {
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w(px(200.))
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
