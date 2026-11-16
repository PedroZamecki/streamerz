//! The Actions page: the action tree beside the open action's triggers and sub-actions. Every row and
//! button comes from the core; a press opens the form, picker, confirmation or operation it names.
use super::{
    form::Form,
    picker::{Chosen, Picked, Picker},
    tree::{Node, Options, PaneEvent, TreePane},
};
use crate::theme::{
    FONT_BODY, HEADER_ROW_HEIGHT, LINE_BODY, PAGE_PAD_X, SIDEBAR_FOLDED_WIDTH, SIDEBAR_WIDTH, SPACE_MD, SPACE_SM,
    SPACE_XS,
};
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    ActiveTheme, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

/// The action node (`a:<id>`) now open in the editor, or an empty text.
pub struct Opened(pub String);
impl EventEmitter<Opened> for ActionsPage {}

#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Actions,
    Triggers,
    Subs,
}

pub struct ActionsPage {
    model: Value,
    route: String,
    actions: Entity<TreePane>,
    triggers: Option<Entity<TreePane>>,
    subs: Option<Entity<TreePane>>,
    /// The pane whose selected row the text under the editor describes.
    detail_pane: Pane,
    open: String,
    form: Option<Entity<Form>>,
    picker_subscription: Option<Subscription>,
    picker_chosen_subscription: Option<Subscription>,
    picker: Option<Entity<Picker>>,
    picker_choice: String,
    /// The button that asked for a picker, and its row.
    asked: Option<(Value, Option<Node>)>,
    select_after: Option<String>,
    error: Option<crate::error::StreamerzError>,
    _subscriptions: Vec<Subscription>,
}

impl ActionsPage {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let actions = cx.new(|cx| {
            TreePane::new(
                "actions",
                model["actions"]["tree"].clone(),
                true,
                Options {
                    in_header: true,
                    ..Options::default()
                },
                window,
                cx,
            )
        });
        let subscription = cx.subscribe_in(&actions, window, |page, _, event: &PaneEvent, window, cx| {
            page.pane_event(Pane::Actions, event, window, cx)
        });
        let mut page = Self {
            detail_pane: Pane::Actions,
            model,
            route,
            actions,
            triggers: None,
            subs: None,
            open: String::new(),
            form: None,
            picker_subscription: None,
            picker_chosen_subscription: None,
            picker: None,
            picker_choice: String::new(),
            asked: None,
            select_after: None,
            error: None,
            _subscriptions: vec![subscription],
        };
        page.sync_editor(window, cx);
        page
    }

    /// The page-wide buttons (Add, Import, Templates), drawn in the page's header.
    pub fn header_actions_view(&self, cx: &App) -> Entity<super::tree::HeaderButtons> {
        self.actions.read(cx).header_view()
    }

    /// What the page says about the open action, or about nothing being open.
    fn default_detail(model: &Value) -> &Value {
        if model["actions"]["editor"].is_object() {
            &model["actions"]["editor"]["detail"]
        } else {
            &model["actions"]["detail"]
        }
    }

    pub fn update_model(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        self.actions
            .update(cx, |pane, cx| pane.set_model(model["actions"]["tree"].clone(), cx));
        self.model = model;
        self.sync_editor(window, cx);
        if let Some(id) = self.select_after.take() {
            for pane in [Some(&self.actions), self.triggers.as_ref(), self.subs.as_ref()]
                .into_iter()
                .flatten()
            {
                pane.update(cx, |pane, cx| pane.select(&id, cx));
            }
        }
        cx.notify();
    }

    /// The panes of the open action exist while the core sends its editor.
    fn sync_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.model["actions"]["editor"].clone();
        if !editor.is_object() {
            self.triggers = None;
            self.subs = None;
            return;
        }
        match &self.triggers {
            Some(pane) => pane.update(cx, |pane, cx| pane.set_model(editor["triggers"].clone(), cx)),
            None => {
                let pane = cx.new(|cx| {
                    TreePane::new(
                        "triggers",
                        editor["triggers"].clone(),
                        false,
                        Options::default(),
                        window,
                        cx,
                    )
                });
                self._subscriptions.push(
                    cx.subscribe_in(&pane, window, |page, _, event: &PaneEvent, window, cx| {
                        page.pane_event(Pane::Triggers, event, window, cx)
                    }),
                );
                self.triggers = Some(pane);
            }
        }
        match &self.subs {
            Some(pane) => pane.update(cx, |pane, cx| pane.set_model(editor["subs"].clone(), cx)),
            None => {
                let pane =
                    cx.new(|cx| TreePane::new("subs", editor["subs"].clone(), true, Options::default(), window, cx));
                self._subscriptions.push(
                    cx.subscribe_in(&pane, window, |page, _, event: &PaneEvent, window, cx| {
                        page.pane_event(Pane::Subs, event, window, cx)
                    }),
                );
                self.subs = Some(pane);
            }
        }
    }

    fn pane_event(&mut self, pane: Pane, event: &PaneEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            PaneEvent::Selected(node) => {
                self.detail_pane = if node.is_some() { pane } else { Pane::Actions };
                if pane == Pane::Actions {
                    self.detail_pane = Pane::Actions;
                    let open = node
                        .as_ref()
                        .filter(|node| node.kind == "action")
                        .map(|node| node.id.clone())
                        .unwrap_or_default();
                    if open != self.open {
                        self.open = open.clone();
                        cx.emit(Opened(open));
                    }
                }
                cx.notify();
            }
            PaneEvent::Activate(node) => {
                let source = match pane {
                    Pane::Actions => Some(&self.actions),
                    Pane::Triggers => self.triggers.as_ref(),
                    Pane::Subs => self.subs.as_ref(),
                };
                let spec = source.and_then(|source| {
                    source.read(cx).model["buttons"]
                        .as_array()
                        .and_then(|buttons| buttons.iter().find(|spec| spec["id"] == "edit"))
                        .cloned()
                });
                if let Some(spec) = spec {
                    self.press(pane, spec, Some(node.clone()), window, cx);
                }
            }
            PaneEvent::Press { spec, node } => self.press(pane, spec.clone(), node.clone(), window, cx),
            PaneEvent::Filter(query) => {
                if pane == Pane::Subs {
                    if let Some(triggers) = &self.triggers {
                        triggers.update(cx, |pane, cx| pane.set_filter(query, cx));
                    }
                }
            }
        }
    }

    /// The action open in the editor, as the core knows it.
    fn action_id(&self) -> String {
        self.model["actions"]["editor"]["actionId"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    fn press(&mut self, pane: Pane, spec: Value, node: Option<Node>, window: &mut Window, cx: &mut Context<Self>) {
        // A row may ask its own question (a group with an overlay connected to it).
        let own = node
            .as_ref()
            .and_then(|node| node.confirm[spec["id"].as_str().unwrap_or_default()].as_str());
        let Some(question) = own.or(spec["confirm"].as_str()) else {
            return self.run(pane, spec, node, None, cx);
        };
        let name = node.as_ref().map(|node| node.label.clone()).unwrap_or_default();
        let question = question.replace("{name}", &name);
        let label = spec["confirmLabel"].as_str().unwrap_or_default().to_owned();
        let danger = spec["danger"] == true;
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let (owner, spec, node) = (owner.clone(), spec.clone(), node.clone());
            let confirm = Button::new("confirm-action")
                .label(label.clone())
                .on_click(move |_, window, cx| {
                    let (spec, node) = (spec.clone(), node.clone());
                    let _ = owner.update(cx, |page, cx| page.run(pane, spec, node, None, cx));
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

    /// Do what a button says: ask for a pick first, open its form, or run its operation.
    fn run(&mut self, pane: Pane, spec: Value, node: Option<Node>, code: Option<String>, cx: &mut Context<Self>) {
        if let (Some(which), None) = (spec["picker"].as_str(), &code) {
            self.asked = Some((spec.clone(), node));
            send(
                json!({"request": "StreamerzCoreView", "domain": "actions_picker", "kind": which,
                "id": format!("{}:picker", self.route)}),
            );
            return;
        }
        let item = node.as_ref().map(|node| node.id.clone()).unwrap_or_default();
        let action = if pane == Pane::Actions {
            String::new()
        } else {
            self.action_id()
        };
        let picked = code.is_some();
        // A pick adds beside the selected row; a row alone is what the form or operation is about.
        let (item, anchor) = if picked {
            (String::new(), item)
        } else {
            (item, String::new())
        };
        if let Some(kind) = spec["form"].as_str() {
            send(
                json!({"request": "StreamerzEntityForm", "kind": kind, "itemId": item, "actionId": action,
                "code": code, "anchor": anchor, "id": format!("{}:form", self.route)}),
            );
        } else if let Some(operation) = spec["op"].as_str() {
            send(
                json!({"request": "StreamerzCoreAction", "domain": "actions", "action": operation, "itemId": item,
                "actionId": action, "code": code, "id": format!("{}:act", self.route)}),
            );
        }
        self.error = None;
        cx.notify();
    }

    fn open_picker(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        let title = model["title"].as_str().unwrap_or_default().to_owned();
        let picker = cx.new(|cx| Picker::new(model, window, cx));
        self.picker = Some(picker.clone());
        self.picker_chosen_subscription = Some(cx.subscribe(&picker, |page, _, chosen: &Chosen, cx| {
            page.picker_choice = chosen.0.clone();
            page.form = None;
            if chosen.0.is_empty() {
                return;
            }
            if let Some((spec, node)) = &page.asked {
                send(json!({"request": "StreamerzEntityForm", "kind": spec["form"],
                    "actionId": page.action_id(), "code": chosen.0,
                    "anchor": node.as_ref().map(|node| node.id.as_str()).unwrap_or_default(),
                    "id": format!("{}:picker-form:{}", page.route, chosen.0)}));
            }
            cx.notify();
        }));
        self.picker_subscription = Some(
            cx.subscribe_in(&picker, window, |page, _, picked: &Picked, window, cx| {
                page.picker = None;
                window.close_dialog(cx);
                if let Some((spec, node)) = page.asked.take() {
                    let pane = if spec["form"].as_str().is_some_and(|kind| kind.contains("trigger")) {
                        Pane::Triggers
                    } else if spec["form"].as_str().is_some() {
                        Pane::Subs
                    } else {
                        Pane::Actions
                    };
                    page.run(pane, spec, node, Some(picked.0.clone()), cx);
                }
            }),
        );
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            crate::controls::dialog::shell(dialog, title.clone(), Picker::width(window), window, cx)
                .child(crate::controls::dialog::body(picker.clone()))
                .footer(picker.read(cx).buttons_view())
                .on_cancel({
                    let owner = owner.clone();
                    move |_, _, cx| {
                        owner
                            .update(cx, |page, cx| {
                                if page.form.as_ref().is_some_and(|form| form.read(cx).pending) {
                                    return false;
                                }
                                page.picker = None;
                                page.asked = None;
                                page.form = None;
                                page.picker_subscription = None;
                                page.picker_chosen_subscription = None;
                                true
                            })
                            .unwrap_or(true)
                    }
                })
        });
    }

    fn open_form(&mut self, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        let title = value["title"].as_str().unwrap_or_default().to_owned();
        let route = format!("{}:save", self.route);
        let form = cx.new(|cx| Form::new(value, route, window, cx));
        self.form = Some(form.clone());
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            crate::controls::dialog::shell(dialog, title.clone(), 600., window, cx)
                .child(crate::controls::dialog::body(form.clone()))
                .footer(form.read(cx).buttons_view())
                .on_cancel({
                    let (form, owner) = (form.clone(), owner.clone());
                    move |_, _, cx| {
                        if form.read(cx).pending {
                            return false;
                        }
                        let _ = owner.update(cx, |page, cx| {
                            page.form = None;
                            cx.notify();
                        });
                        true
                    }
                })
        });
    }

    /// The core downloaded pictures: an open form's picture pickers ask for theirs again.
    pub fn images_arrived(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = &self.form {
            form.update(cx, |form, cx| form.images_arrived(cx));
        }
    }

    /// A reply to one of this page's requests. Returns whether the page should be read again.
    pub fn apply(&mut self, operation: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(choice) = operation.strip_prefix("picker-form:") {
            if choice != self.picker_choice || self.picker.is_none() {
                return false;
            }
            if let Some(err) = crate::error::StreamerzError::from_response(&value) {
                self.error = Some(err);
            } else if let Some(picker) = &self.picker {
                let form = cx.new(|cx| Form::new(value, format!("{}:save", self.route), window, cx));
                self.form = Some(form.clone());
                picker.update(cx, |picker, cx| picker.set_form(form, cx));
            }
            cx.notify();
            return false;
        }
        // A form's picture picker: its pictures or its reloaded list (`gallery:<field>:images|choices`).
        if let Some((index, what)) = operation.strip_prefix("gallery:").and_then(|rest| rest.split_once(':')) {
            if let (Some(form), Ok(index)) = (&self.form, index.parse()) {
                form.update(cx, |form, cx| form.gallery_reply(index, what, &value, cx));
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
        match operation {
            "picker" => self.open_picker(value, window, cx),
            "form" => self.open_form(value, window, cx),
            "json" => {
                if let Some(form) = &self.form {
                    form.update(cx, |form, cx| {
                        form.show_json(value["json"].as_str().unwrap_or_default(), window, cx)
                    });
                }
            }
            "save" | "act" => {
                if operation == "save" {
                    self.picker = None;
                    self.asked = None;
                    self.form = None;
                    window.close_dialog(cx);
                }
                if value["values"].is_object() {
                    super::list::show_values(&value["values"], window, cx);
                }
                if let Some(text) = value["copy"].as_str() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                    if let Some(message) = value["message"].as_str() {
                        window.push_notification(message.to_owned(), cx);
                    }
                }
                self.error = None;
                self.select_after = value["saved"].as_str().filter(|id| !id.is_empty()).map(str::to_owned);
                return true;
            }
            _ => {}
        }
        false
    }
}

impl Render for ActionsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = f32::from(window.viewport_size().width);
        let content = viewport
            - if viewport < 700. {
                SIDEBAR_FOLDED_WIDTH
            } else {
                SIDEBAR_WIDTH
            };
        let stacked = content < 760.;
        let idle = !self.model["actions"]["editor"].is_object();
        if idle {
            return div()
                .size_full()
                .flex()
                .gap(px(SPACE_MD))
                .child(div().flex_1().min_w_0().child(self.actions.clone()))
                .into_any_element();
        }
        // The frame's left and right padding is outside the page: widths follow from it.
        let inner = content - 2. * PAGE_PAD_X;
        let (left_width, right_width) = if stacked {
            (inner, inner)
        } else {
            (340., inner - 340. - SPACE_MD)
        };
        self.actions.update(cx, |pane, cx| {
            pane.set_options(
                Options {
                    width: left_width,
                    icons_first: false,
                    header: true,
                    in_header: true,
                    ..Options::default()
                },
                cx,
            )
        });
        if let Some(pane) = &self.triggers {
            pane.update(cx, |pane, cx| {
                pane.set_options(
                    Options {
                        width: right_width,
                        idle,
                        ..Options::default()
                    },
                    cx,
                )
            });
        }
        if let Some(pane) = &self.subs {
            pane.update(cx, |pane, cx| {
                pane.set_options(
                    Options {
                        width: right_width,
                        idle,
                        header: true,
                        ..Options::default()
                    },
                    cx,
                )
            });
        }
        let editor = &self.model["actions"]["editor"];
        let editor = if editor.is_object() {
            editor
        } else {
            &self.model["actions"]["idle"]
        };
        // What the selected trigger or sub-action is, else the action itself.
        let selected = match self.detail_pane {
            Pane::Actions => None,
            Pane::Triggers => self
                .triggers
                .as_ref()
                .and_then(|pane| pane.read(cx).selected().cloned()),
            Pane::Subs => self.subs.as_ref().and_then(|pane| pane.read(cx).selected().cloned()),
        };
        let detail_of = |detail: &Value| {
            (
                detail["title"].as_str().unwrap_or_default().to_owned(),
                detail["text"].as_str().unwrap_or_default().to_owned(),
            )
        };
        let detail = match &selected {
            Some(node) if node.detail.is_object() => detail_of(&node.detail),
            _ => detail_of(Self::default_detail(&self.model)),
        };
        let mut right = div().flex_1().min_w_0().min_h_0().flex().flex_col().gap(px(SPACE_SM));
        if let Some(result) = editor["lastTest"].as_object() {
            let id = result.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
            let text = result
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let hint = result
                .get("outcome")
                .and_then(|v| v["error"].as_str())
                .or_else(|| result.get("hint").and_then(Value::as_str))
                .unwrap_or_default()
                .to_owned();
            let label = result
                .get("details")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            right =
                right
                    .child(super::notice::info(&text, cx))
                    .child(super::notice::info(&hint, cx))
                    .child(Button::new("last-test-details").label(label).on_click(cx.listener(
                        move |page, _, _, _cx| {
                            send(
                                json!({"request": "StreamerzCoreAction", "domain": "actions", "action": "run_details",
                        "itemId": id, "id": format!("{}:act", page.route)}),
                            );
                        },
                    )));
        }
        if let (Some(triggers), Some(subs)) = (&self.triggers, &self.subs) {
            right = right
                .child({
                    let title = editor["title"].as_str().unwrap_or_default().to_owned();
                    let title_tip = title.clone();
                    div()
                        .id("action-editor-title")
                        .h(px(HEADER_ROW_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .truncate()
                        .font_weight(FontWeight::BOLD)
                        .text_color(crate::theme::strong(cx))
                        .child(title)
                        .when(!title_tip.is_empty(), |el| {
                            el.tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(title_tip.clone()).build(window, cx)
                            })
                        })
                })
                .child(
                    div()
                        .h(px(HEADER_ROW_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(tr("actions.triggers_heading")),
                )
                .child(
                    div()
                        .h(px(if stacked { 88. } else { 152. }))
                        .flex_shrink_0()
                        .child(triggers.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(if stacked { 100. } else { 200. }))
                        .child(subs.clone()),
                )
                .when(!idle, |right| {
                    right.child(
                        div()
                            .id("details")
                            .flex_shrink_0()
                            .min_h(px(44.))
                            .max_h(px(120.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_XS))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(crate::theme::strong(cx))
                                    .child(detail.0.clone()),
                            )
                            .child(
                                div()
                                    .pl(px(SPACE_XS))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(super::markdown::inline("action-detail", detail.1.clone())),
                            ),
                    )
                });
        }
        let left = div()
            .when(stacked, |left| left.h(px(210.)).w_full().flex_shrink_0())
            .when(!stacked, |left| left.w(px(340.)).h_full().flex_shrink_0())
            .child(self.actions.clone());
        let mut column = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .min_h_0()
            .flex()
            .flex_col()
            .text_size(px(FONT_BODY))
            .line_height(px(LINE_BODY))
            .gap(px(SPACE_SM));
        if let Some(err) = &self.error {
            column = column.child(super::notice::error(err, cx));
        }
        if stacked {
            // Too narrow for two panes: the whole page scrolls instead of squeezing the editor.
            return div()
                .relative()
                .size_full()
                .child(
                    column.child(
                        div().id("actions-scroll").flex_1().min_h_0().overflow_y_scroll().child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(SPACE_MD))
                                .child(left)
                                .child(right.min_h(px(424.)).flex_none()),
                        ),
                    ),
                )
                .into_any_element();
        }
        div()
            .relative()
            .size_full()
            .child(
                column.child(
                    div()
                        .size_full()
                        .min_h_0()
                        .flex()
                        .gap(px(SPACE_MD))
                        .child(left)
                        .child(right),
                ),
            )
            .into_any_element()
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::ActionsPage;
    use crate::controls::tree::tests::{is_visible_empty, matching_ids, search_input};
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{
        div, px, size, AppContext, Bounds, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window,
        WindowBounds, WindowOptions,
    };
    use serde_json::json;

    struct ActionsHost(Entity<ActionsPage>);
    impl Render for ActionsHost {
        fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    #[gpui_kit::test]
    fn searching_actions_does_not_filter_triggers_while_searching_subs_does(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::apply(true, cx);
        });
        let model = json!({
            "actions": {
                "tree": {
                    "title": "Actions",
                    "searchHint": "Search actions…",
                    "empty": "No actions.",
                    "nodes": [
                        {"id": "a:1", "kind": "action", "label": "Shoutout"},
                        {"id": "a:2", "kind": "action", "label": "Raid"}
                    ],
                    "buttons": []
                },
                "editor": {
                    "actionId": "1",
                    "title": "Shoutout",
                    "detail": {"title": "Shoutout", "text": "2 triggers, 2 sub-actions"},
                    "triggers": {
                        "title": "Triggers",
                        "empty": "No triggers.",
                        "nodes": [
                            {"id": "t:1", "kind": "trigger", "label": "Chat Message"},
                            {"id": "t:2", "kind": "trigger", "label": "Follow"}
                        ],
                        "buttons": []
                    },
                    "subs": {
                        "title": "Sub-actions",
                        "searchHint": "Search sub-actions…",
                        "empty": "No sub-actions.",
                        "nodes": [
                            {"id": "s:1", "kind": "sub", "label": "Send Message"},
                            {"id": "s:2", "kind": "sub", "label": "Play Sound"}
                        ],
                        "buttons": []
                    }
                }
            }
        });
        let slot: std::rc::Rc<std::cell::RefCell<Option<Entity<ActionsPage>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let s = slot.clone();
        let window = cx.update(|cx| {
            let bounds = Bounds {
                origin: gpui_kit::Point::default(),
                size: size(px(1100.), px(700.)),
            };
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    let page = cx.new(|cx| ActionsPage::new(model, "actions".into(), window, cx));
                    *s.borrow_mut() = Some(page.clone());
                    cx.new(|_| ActionsHost(page))
                },
            )
            .expect("open test window");
            window.downcast::<gpui_kit::base::Root>().expect("root window")
        });
        let page = slot.borrow().clone().expect("actions page created");

        let no_matches = crate::i18n::tr("lists.no_matches");
        let (subs_pane, triggers_pane, subs_search) = cx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let actions_search = search_input(page.read(cx).actions.read(cx)).expect("actions search");
                let subs_pane = page.read(cx).subs.clone().expect("subs pane");
                let triggers_pane = page.read(cx).triggers.clone().expect("triggers pane");
                let subs_search = search_input(subs_pane.read(cx)).expect("subs search");

                // Searching Actions must filter Actions only, leaving Triggers intact.
                actions_search.update(cx, |input, cx| input.focus(window, cx));
                window.input("Raid", cx);
                (subs_pane, triggers_pane, subs_search)
            })
            .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(matching_ids(page.read(cx).actions.read(cx), cx), vec!["a:2"]);
            assert_eq!(matching_ids(triggers_pane.read(cx), cx), vec!["t:1", "t:2"]);
            assert!(!is_visible_empty(triggers_pane.read(cx)));
            assert!(window.try_find_by_label(no_matches).is_none());

            // Searching Sub-actions filters Sub-actions and the open action's Triggers.
            subs_search.update(cx, |input, cx| input.focus(window, cx));
            window.input("Chat", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(matching_ids(subs_pane.read(cx), cx).is_empty());
            assert!(is_visible_empty(subs_pane.read(cx)));
            assert!(window.try_find_by_label(no_matches).is_some());
            assert_eq!(matching_ids(triggers_pane.read(cx), cx), vec!["t:1"]);
        })
        .unwrap();
    }
}
