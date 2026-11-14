//! The Actions page: the action tree beside the open action's triggers and sub-actions. Every row and
//! button comes from the core; a press opens the form, picker, confirmation or operation it names.
use super::{
    form::{Cancelled, Form},
    picker::{Picked, Picker},
    tree::{Node, Options, PaneEvent, TreePane},
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
    form_subscription: Option<Subscription>,
    picker_subscription: Option<Subscription>,
    /// The button that asked for a picker, and its row.
    asked: Option<(Value, Option<Node>)>,
    select_after: Option<String>,
    error: String,
    _subscriptions: Vec<Subscription>,
}

impl ActionsPage {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let actions = cx.new(|cx| {
            TreePane::new(
                "actions",
                model["actions"]["tree"].clone(),
                true,
                Options::default(),
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
            form_subscription: None,
            picker_subscription: None,
            asked: None,
            select_after: None,
            error: String::new(),
            _subscriptions: vec![subscription],
        };
        page.sync_editor(window, cx);
        page
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
        let mut editor = self.model["actions"]["editor"].clone();
        // Nothing open: the same panes, empty and off (1.6.0).
        if !editor.is_object() {
            editor = self.model["actions"]["idle"].clone();
        }
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
                if let Some(triggers) = &self.triggers {
                    triggers.update(cx, |pane, cx| pane.set_filter(query, cx));
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
        window.open_dialog(cx, move |dialog, _, cx| {
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
        self.error.clear();
        cx.notify();
    }

    fn open_picker(&mut self, model: Value, window: &mut Window, cx: &mut Context<Self>) {
        let title = model["title"].as_str().unwrap_or_default().to_owned();
        let picker = cx.new(|cx| Picker::new(model, window, cx));
        self.picker_subscription = Some(
            cx.subscribe_in(&picker, window, |page, _, picked: &Picked, window, cx| {
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
        window.open_dialog(cx, move |dialog, window, cx| {
            crate::controls::dialog::shell(dialog, title.clone(), Picker::width(window), cx)
                .child(crate::controls::dialog::body(picker.clone()))
                .footer(picker.read(cx).buttons_view())
        });
    }

    fn open_form(&mut self, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        let title = value["title"].as_str().unwrap_or_default().to_owned();
        let route = format!("{}:save", self.route);
        let form = cx.new(|cx| Form::new(value, route, window, cx));
        self.form_subscription = Some(cx.subscribe(&form, |page, _, _: &Cancelled, cx| {
            page.form = None;
            page.form_subscription = None;
            cx.notify();
        }));
        self.form = Some(form.clone());
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            crate::controls::dialog::shell(dialog, title.clone(), 600., cx)
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
                            page.form_subscription = None;
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
        // A form's picture picker: its pictures or its reloaded list (`gallery:<field>:images|choices`).
        if let Some((index, what)) = operation.strip_prefix("gallery:").and_then(|rest| rest.split_once(':')) {
            if let (Some(form), Ok(index)) = (&self.form, index.parse()) {
                form.update(cx, |form, cx| form.gallery_reply(index, what, &value, cx));
            }
            return false;
        }
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
                    self.form = None;
                    self.form_subscription = None;
                    window.close_dialog(cx);
                }
                if let Some(text) = value["copy"].as_str() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                    if let Some(message) = value["message"].as_str() {
                        window.push_notification(message.to_owned(), cx);
                    }
                }
                self.error.clear();
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
        let content = viewport - if viewport < 700. { 52. } else { 176. };
        let stacked = content < 760.;
        let idle = !self.model["actions"]["editor"].is_object();
        // The page's own padding is 8 here (the shell's is 10): widths follow from it.
        let inner = content - 16.;
        let (left_width, right_width) = if stacked { (inner, inner) } else { (340., inner - 348.) };
        self.actions.update(cx, |pane, cx| {
            pane.set_options(
                Options {
                    width: left_width,
                    icons_first: true,
                    header: true,
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
        let mut right = div().flex_1().min_w_0().min_h_0().flex().flex_col().gap(px(4.));
        if let (Some(triggers), Some(subs)) = (&self.triggers, &self.subs) {
            right = right
                .child({
                    let title = editor["title"].as_str().unwrap_or_default().to_owned();
                    let title_tip = title.clone();
                    div()
                        .id("action-editor-title")
                        .h(px(24.))
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
                        .h(px(22.))
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
                            .h(px(60.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(crate::theme::strong(cx))
                                    .child(detail.0.clone()),
                            )
                            .child(
                                div()
                                    .pl(px(3.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(detail.1.clone()),
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
            .left(px(-2.))
            .right(px(-2.))
            .bottom(px(-2.))
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2();
        if !self.error.is_empty() {
            column = column.child(super::notice::error(&self.error, cx));
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
                                .gap_2()
                                .child(left)
                                .child(right.h(px(424.)).flex_none()),
                        ),
                    ),
                )
                .into_any_element();
        }
        div()
            .relative()
            .size_full()
            .child(column.child(div().size_full().min_h_0().flex().gap_2().child(left).child(right)))
            .into_any_element()
    }
}
