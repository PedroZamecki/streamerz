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
        let table = (model["presentation"] != "settings"
            && model["presentation"] != "connections"
            && model["presentation"] != "cards")
            .then(|| {
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
            && !(matches!(action, Action::Toggle) && shown["switchToggle"] == true)
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
                // A row may ask its own question (the last page of an overlay, one with actions).
                let own = record
                    .as_ref()
                    .and_then(|row| row["confirm"][spec["id"].as_str().unwrap_or_default()].as_str());
                match own.or(spec["confirm"].as_str()).filter(|_| spec["confirm"].is_string()) {
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
                                        .on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx)),
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
                window.open_dialog(cx, move |dialog, _, cx| {
                    let kind = kind.clone(); let id = id.clone(); let route = route.clone();
                    let buttons = div().id("delete-confirmation").test_support().role(Role::Group).aria_label(confirm_label.clone()).flex().justify_end().gap_2()
                        .child(Button::new("cancel-delete").label(tr("dialogs.cancel")).on_click(|_, window, cx| crate::controls::dialog::dismiss(window, cx)))
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
    fn top_buttons(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<Button> {
        let header_add = self.model["headerAdd"] == true;
        let actions: Vec<Action> = Action::base_actions(&self.model)
            .into_iter()
            .filter(|action| {
                self.on_top(*action)
                    && !(header_add && matches!(action, Action::Add))
                    && (!self.hide_disabled(*action) || action.enabled(&self.model, None))
            })
            .collect();
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
        let room = self.table.as_ref().map_or(0., |table| table.read(cx).delegate().fit);
        let room = if room > 0. {
            room
        } else {
            f32::from(window.viewport_size().width) - 200.
        };
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
                    .h(px(26.))
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
    /// The grid of buttons under the table (a page may draw it later, after another section).
    pub fn button_rows(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        if self.is_connections() || self.is_cards() {
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
        let mut buttons = div().flex().flex_col().gap(px(6.)).w_full().mt(px(2.));
        if self.model["headerAdd"] == true && self.has_connection() {
            if self.model["rows"].as_array().is_none_or(Vec::is_empty) {
                return None;
            }
            let room = self.table.as_ref().map_or(0., |table| table.read(cx).delegate().fit);
            let room = if room > 0. {
                room
            } else {
                f32::from(window.viewport_size().width) - 200.
            };
            let fold = room < 360.;
            let mut line = div().flex().items_center().gap_2().w_full();
            for action in actions.iter().copied() {
                if matches!(action, Action::Delete) {
                    line = line.child(div().flex_1());
                }
                let label = action.row_label(&self.model, selected);
                line = line.child(
                    Button::new(SharedString::from(format!("conn-{}", action.id(&self.model))))
                        .small()
                        .h(px(26.))
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
            let mut line = div().flex().items_center().gap(px(6.)).w_full();
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
                        .h(px(28.))
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
                // Each button in a cell of its own: every cell of every row is as wide as the others.
                line = line.child(
                    div().flex_1().min_w_0().child(
                        Button::new(id)
                            .small()
                            .h(px(28.))
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
        let mut body = div()
            .w_full()
            .when(!self.is_connections() && !self.is_cards(), |el| el.h_full())
            .flex()
            .flex_col()
            .gap(px(2.))
            .pt(px(2.));
        let guide = self.model["guide"].as_str().unwrap_or_default();
        // A page's introduction and its state line, above the first table.
        if !self.lead_outside {
            if let Some(lead) = self.intro_block(cx) {
                body = body.child(lead);
            }
        }
        let options = self.model["filter"]["options"].as_array().cloned().unwrap_or_default();
        let mut top = self.top_buttons(window, cx);
        let titled = !self.model["title"].as_str().unwrap_or_default().is_empty();
        let show_heading =
            titled && ((!guide.is_empty() && self.model["titleless"] != true) || self.model["showTitle"] == true);
        let header_add = self.model["headerAdd"] == true;
        let header_top = show_heading
            && !header_add
            && self.search.is_none()
            && options.is_empty()
            && self.show.is_none()
            && !top.is_empty();
        if show_heading {
            let row_count = self.model["rows"].as_array().map_or(0, Vec::len);
            let has_rows = row_count > 0;
            let mut heading = div()
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(if header_add || header_top { 28. } else { 24. }))
                .pl(px(2.))
                .when(header_add || header_top, |row| row.justify_between().w_full());
            let mut left = div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
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
                .child(self.model["title"].as_str().unwrap_or_default().to_owned());
            if header_add && self.has_connection() && has_rows {
                let (icon, color) =
                    super::health::appearance(self.model["connection"]["health"].as_str().unwrap_or_default(), cx);
                left = left.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(px(99.))
                        .bg(rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }))
                        .border_1()
                        .border_color(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .text_size(px(12.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(color)
                        .child(Icon::empty().path(format!("icons/{icon}.svg")).size(px(12.)))
                        .child(self.model["connection"]["text"].as_str().unwrap_or_default().to_owned()),
                );
            }
            heading = heading.child(left);
            if header_add {
                let add_label = Action::Add.label(&self.model);
                if self.is_connections() || self.is_cards() {
                    let mut actions = div().flex().items_center().gap(px(6.)).flex_shrink_0();
                    if self.is_connections() && row_count > 1 {
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
                                .w(px(26.))
                                .h(px(24.))
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
                            .w(px(26.))
                            .h(px(24.))
                            .icon(Icon::empty().path("icons/plus.svg").size(px(13.)))
                            .tooltip(add_label.clone())
                            .accessibility_label(add_label)
                            .on_click(cx.listener(|list, _, window, cx| list.action(Action::Add, None, window, cx))),
                    );
                    heading = heading.child(actions);
                } else {
                    heading = heading.child(
                        Button::new("header-add")
                            .small()
                            .h(px(26.))
                            .flex_shrink_0()
                            .label(add_label.clone())
                            .accessibility_label(add_label)
                            .icon(Action::Add.icon(&self.model))
                            .on_click(cx.listener(|list, _, window, cx| list.action(Action::Add, None, window, cx))),
                    );
                }
            } else if header_top {
                heading = heading.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .flex_shrink_0()
                        .children(std::mem::take(&mut top)),
                );
            }
            body = body.child(heading);
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
            body = body.gap_1();
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
                        .h(px(32.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }))
                        .border_1()
                        .border_color(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .hover(|style| style.bg(rgb(if cx.theme().is_dark() { 0x222226 } else { 0xebebee })))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .min_w_0()
                                .flex_1()
                                .child(
                                    Icon::empty()
                                        .path(format!("icons/{proto_icon}.svg"))
                                        .size(px(15.))
                                        .text_color(if enabled {
                                            cx.theme().foreground
                                        } else {
                                            crate::theme::quiet(cx)
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
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
                                            .text_size(px(12.5))
                                            .text_color(if enabled {
                                                rgb(if cx.theme().is_dark() { 0x93c5fd } else { 0x1d4ed8 }).into()
                                            } else {
                                                crate::theme::quiet(cx)
                                            })
                                            .child(location),
                                    )
                                })
                                .when(has_auth, |row_el| {
                                    row_el.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(3.))
                                            .px(px(5.))
                                            .py(px(1.))
                                            .rounded(px(3.))
                                            .bg(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                                            .text_color(crate::theme::quiet(cx))
                                            .child(Icon::empty().path("icons/lock.svg").size(px(11.)))
                                            .child(div().text_size(px(11.)).child(
                                                tr("lists.servers.password_tag").trim_start_matches(", ").to_owned(),
                                            )),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    Button::new(("toggle-setting", index))
                                        .small()
                                        .w(px(26.))
                                        .h(px(24.))
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
                                        .w(px(26.))
                                        .h(px(24.))
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
            body = body.gap_1();
            let rows = self.model["rows"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                body = body.child(
                    div()
                        .h(px(36.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }))
                        .border_1()
                        .border_color(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(13.))
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
                        .gap_2()
                        .h(px(34.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }))
                        .border_1()
                        .border_color(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .hover(|style| style.bg(rgb(if cx.theme().is_dark() { 0x222226 } else { 0xebebee })))
                        .child(
                            div()
                                .id(("conn-info", index))
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .min_w_0()
                                .flex_1()
                                .on_click(cx.listener(move |list, event: &ClickEvent, window, cx| {
                                    if event.click_count() > 1 {
                                        list.action(Action::Edit, Some(index), window, cx);
                                    }
                                }))
                                .child(
                                    img(format!("brands/{brand}.svg"))
                                        .id(("conn-brand", index))
                                        .size(px(15.))
                                        .flex_shrink_0(),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(13.))
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
                                            .text_size(px(12.5))
                                            .text_color(if connected {
                                                rgb(if cx.theme().is_dark() { 0x93c5fd } else { 0x1d4ed8 }).into()
                                            } else {
                                                crate::theme::quiet(cx)
                                            })
                                            .child(address),
                                    )
                                })
                                .when(secret, |row_el| {
                                    row_el.child(
                                        div()
                                            .id(("conn-secret", index))
                                            .flex()
                                            .items_center()
                                            .flex_shrink_0()
                                            .px(px(5.))
                                            .py(px(2.))
                                            .rounded(px(3.))
                                            .bg(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                                            .text_color(crate::theme::quiet(cx))
                                            .child(Icon::empty().path("icons/lock.svg").size(px(11.)))
                                            .tooltip(move |window, cx| {
                                                gpui_kit::component::tooltip::Tooltip::new(secret_tip.clone())
                                                    .build(window, cx)
                                            }),
                                    )
                                })
                                .when(reconnect, |row_el| {
                                    let tip = retry_tip.clone();
                                    row_el.child(
                                        div()
                                            .id(("conn-retry", index))
                                            .flex()
                                            .items_center()
                                            .gap(px(3.))
                                            .flex_shrink_0()
                                            .px(px(5.))
                                            .py(px(1.))
                                            .rounded(px(3.))
                                            .bg(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                                            .text_color(crate::theme::quiet(cx))
                                            .child(Icon::empty().path("icons/refresh-cw.svg").size(px(11.)))
                                            .child(div().text_size(px(11.)).child(format!("{retry_interval}s")))
                                            .tooltip(move |window, cx| {
                                                gpui_kit::component::tooltip::Tooltip::new(tip.clone())
                                                    .build(window, cx)
                                            }),
                                    )
                                })
                                .when(is_default, |row_el| {
                                    row_el.child(
                                        div()
                                            .id(("conn-default", index))
                                            .flex()
                                            .items_center()
                                            .flex_shrink_0()
                                            .px(px(5.))
                                            .py(px(2.))
                                            .rounded(px(3.))
                                            .bg(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                                            .text_color(crate::theme::quiet(cx))
                                            .child(Icon::empty().path("icons/star-filled.svg").size(px(11.)))
                                            .tooltip(move |window, cx| {
                                                gpui_kit::component::tooltip::Tooltip::new(
                                                    tr("lists.default_badge").to_owned(),
                                                )
                                                .build(window, cx)
                                            }),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .flex_shrink_0()
                                .child(
                                    Button::new(("conn-toggle", index))
                                        .small()
                                        .w(px(26.))
                                        .h(px(24.))
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
                                        .w(px(26.))
                                        .h(px(24.))
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
                                        .w(px(26.))
                                        .h(px(24.))
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
                                        .w(px(26.))
                                        .h(px(24.))
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
            body = body.gap_1();
            let rows = self.model["rows"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                let empty_icon = self.model["icon"].as_str().unwrap_or("folder");
                let empty_model = self.model.clone();
                let empty_owner = cx.entity().downgrade();
                body = body.child(
                    div()
                        .id("cards-empty")
                        .h(px(36.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }))
                        .border_1()
                        .border_color(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(13.))
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
                    .gap(px(10.))
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
                                .size(px(15.))
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
                            .size(px(15.))
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
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
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
                                .text_size(px(12.5))
                                .text_color(if enabled {
                                    rgb(if cx.theme().is_dark() { 0x93c5fd } else { 0x1d4ed8 }).into()
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
                    let mut pill = div()
                        .id(("card-badge", index * 16 + b_idx))
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .flex_shrink_0()
                        .px(px(5.))
                        .py(px(if b_text.is_empty() { 2. } else { 1. }))
                        .rounded(px(3.))
                        .bg(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .text_color(crate::theme::quiet(cx));
                    if !b_icon.is_empty() {
                        pill = pill.child(Icon::empty().path(format!("icons/{b_icon}.svg")).size(px(11.)));
                    }
                    if !b_text.is_empty() {
                        pill = pill.child(div().text_size(px(11.)).child(b_text));
                    }
                    if !b_tip.is_empty() {
                        let tip = b_tip.clone();
                        pill = pill.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        });
                    }
                    info = info.child(pill);
                }
                let mut right = div().flex().items_center().gap(px(6.)).flex_shrink_0();
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
                            .w(px(26.))
                            .h(px(24.))
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
                            .w(px(26.))
                            .h(px(24.))
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
                            .w(px(26.))
                            .h(px(24.))
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
                        .gap_2()
                        .h(px(34.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(if cx.theme().is_dark() { 0x18181b } else { 0xf4f4f5 }))
                        .border_1()
                        .border_color(rgb(if cx.theme().is_dark() { 0x27272a } else { 0xe4e4e7 }))
                        .hover(|style| style.bg(rgb(if cx.theme().is_dark() { 0x222226 } else { 0xebebee })))
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
            if self.search.is_none() && !top.is_empty() {
                line = line.child(div().flex_1());
            }
            body = body.child(line.children(top));
        }
        if let Some(table) = &self.table {
            body = body.child(
                // The header is a quiet line above a rounded box (drawn over the rows, no fill).
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
                            // Columns shrink to the table's width (rows::fit_widths): never a sideways bar.
                            .scrollbar_visible(true, false)
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
        if self.model["connection"].is_object() && self.model["headerAdd"] != true {
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
