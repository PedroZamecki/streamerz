//! Shared schema-driven forms. Validation and type conversion stay in the core.
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::Button,
    checkbox::Checkbox,
    input::{Input, InputState, Textarea, TextareaState},
    select::{Select, SelectEvent, SelectState},
    ActiveTheme, Disableable, IndexPath, Sizable, WindowExt,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{json, Value};

enum Field {
    Text(Entity<InputState>),
    Multi(Entity<TextareaState>),
    Bool(bool),
    Pick(Entity<SelectState<Vec<String>>>),
    /// A field that names something else: its ids and names come from the core. With an input it
    /// also takes typed text; without one it takes only an id of the list (or none).
    Choice {
        select: Entity<SelectState<Vec<String>>>,
        input: Option<Entity<InputState>>,
        ids: Vec<String>,
    },
}
pub struct Cancelled;
pub struct Form {
    pub model: Value,
    fields: Vec<Field>,
    route: String,
    json_editor: Option<Entity<TextareaState>>,
    pub error: String,
    pub pending: bool,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<Cancelled> for Form {}
impl Form {
    pub fn new(model: Value, route: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        let fields = model["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, field)| {
                let value = &field["value"];
                let kind = field["widget"]["kind"].as_str().unwrap_or_default();
                if let Some(choices) = field["choices"].as_array() {
                    let strict = field["strict"] == true;
                    let current = value.as_str().unwrap_or_default();
                    let mut ids: Vec<String> = Vec::new();
                    let mut names: Vec<String> = Vec::new();
                    if strict {
                        ids.push(String::new());
                        names.push(tr("forms.none").to_owned());
                    }
                    for choice in choices {
                        ids.push(choice["id"].as_str().unwrap_or_default().to_owned());
                        names.push(choice["label"].as_str().unwrap_or_default().to_owned());
                    }
                    // A stored id the list no longer has stays selectable, as itself.
                    if strict && !ids.iter().any(|id| id == current) {
                        ids.push(current.to_owned());
                        names.push(current.to_owned());
                    }
                    let selected = ids.iter().position(|id| id == current).map(IndexPath::new);
                    let select = cx.new(|cx| SelectState::new(names, selected, window, cx));
                    let input =
                        (!strict).then(|| cx.new(|cx| InputState::new(window, cx).default_value(current.to_owned())));
                    if let Some(input) = input.clone() {
                        let chosen = ids.clone();
                        subscriptions.push(cx.subscribe_in(
                            &select,
                            window,
                            move |_, select, event: &SelectEvent<Vec<String>>, window, cx| {
                                if let SelectEvent::Confirm(Some(_)) = event {
                                    if let Some(id) =
                                        select.read(cx).selected_index(cx).and_then(|at| chosen.get(at.row))
                                    {
                                        input.update(cx, |input, cx| input.set_value(id.clone(), window, cx));
                                    }
                                }
                            },
                        ));
                    }
                    let _ = index;
                    return Field::Choice { select, input, ids };
                }
                match kind {
                    "bool" | "bit" => Field::Bool(value.as_bool().unwrap_or(false)),
                    "pick" => {
                        let options = field["widget"]["options"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect::<Vec<_>>();
                        let selected = value
                            .as_u64()
                            .and_then(|value| usize::try_from(value).ok())
                            .filter(|index| *index < options.len())
                            .map(IndexPath::new);
                        Field::Pick(cx.new(|cx| SelectState::new(options, selected, window, cx)))
                    }
                    "multi" | "lines" | "map" => Field::Multi(cx.new(|cx| {
                        TextareaState::new(window, cx).default_value(value.as_str().unwrap_or_default().to_owned())
                    })),
                    _ => Field::Text(cx.new(|cx| {
                        InputState::new(window, cx)
                            .default_value(value.as_str().unwrap_or_default().to_owned())
                            .masked(kind == "secret")
                    })),
                }
            })
            .collect();
        // A type that has no form opens as JSON.
        let json_editor = (model["jsonOnly"] == true).then(|| {
            cx.new(|cx| {
                TextareaState::new(window, cx).default_value(model["jsonText"].as_str().unwrap_or_default().to_owned())
            })
        });
        Self {
            model,
            fields,
            route,
            json_editor,
            error: String::new(),
            pending: false,
            _subscriptions: subscriptions,
        }
    }
    pub fn show_json(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.json_editor = Some(cx.new(|cx| TextareaState::new(window, cx).default_value(text.to_owned())));
        self.pending = false;
        self.error.clear();
        cx.notify();
    }
    fn submit(&mut self, preview: bool, cx: &mut Context<Self>) {
        let mut request = json!({"request": if preview { "StreamerzEntityDraft" } else { "StreamerzEditEntity" }, "kind": self.model["kind"], "itemId": self.model["item"]["id"].as_str().unwrap_or_default(), "id": if preview { self.route.replace(":save", ":json") } else { self.route.clone() }});
        if let Some(editor) = &self.json_editor {
            request["json"] = editor.read(cx).value().to_string().into();
        } else {
            request["changes"] = json!(self.changes(cx));
        }
        send(request);
        self.pending = true;
        self.error.clear();
        cx.notify();
    }
    fn changes(&self, cx: &App) -> Vec<Value> {
        self.fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| {
                let value = match field {
                    Field::Bool(value) => json!(value),
                    Field::Text(input) => json!(input.read(cx).value().to_string()),
                    Field::Multi(input) => json!(input.read(cx).value().to_string()),
                    Field::Pick(input) => match input.read(cx).selected_index(cx) {
                        Some(index) => json!(index.row),
                        None => return None,
                    },
                    Field::Choice {
                        select,
                        input: Some(input),
                        ..
                    } => {
                        let _ = select;
                        json!(input.read(cx).value().to_string())
                    }
                    Field::Choice { select, ids, .. } => match select.read(cx).selected_index(cx) {
                        Some(at) => json!(ids.get(at.row).cloned().unwrap_or_default()),
                        None => return None,
                    },
                };
                (value != self.model["fields"][index]["value"]).then(|| json!({"field": index, "value": value}))
            })
            .collect()
    }
}
impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("form-fields")
            .flex()
            .flex_col()
            .gap_1()
            .overflow_y_scroll()
            .flex_1()
            .min_h_0();
        let mut wanted = 0.;
        if let Some(hint) = self.model["hint"].as_str().filter(|hint| !hint.is_empty()) {
            body = body.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(hint.to_owned()),
            );
            wanted += 18. * (hint.len() as f32 / 70.).ceil().max(1.) + 8.;
        }
        if !self.error.is_empty() {
            body = body.child(super::notice::error(&self.error, cx));
            wanted += 40.;
        }
        // Why part of the form is unavailable, in quiet text.
        for note in self.model["notes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            body = body.child(div().text_color(cx.theme().muted_foreground).child(note.to_owned()));
            wanted += 24.;
        }
        if let Some(editor) = &self.json_editor {
            wanted += 440.;
            body = body.child(Textarea::new(editor).aria_label(tr("btn.edit_json")).h_full());
        } else {
            let mut index = 0;
            while index < self.fields.len() {
                if matches!(self.fields[index], Field::Bool(_)) {
                    let mut row = div().flex().items_center().gap_2().h(px(26.)).flex_shrink_0();
                    for _ in 0..2 {
                        let Some(Field::Bool(checked)) = self.fields.get(index) else {
                            break;
                        };
                        let field_index = index;
                        let label = self.model["fields"][index]["label"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        // A platform's check box carries its logo; a disabled one says why on hover.
                        let brand = self.model["fields"][index]["brand"].as_str().map(str::to_owned);
                        let tip = self.model["fields"][index]["tip"]
                            .as_str()
                            .map(|tip| SharedString::from(tip.to_owned()));
                        row = row.child(
                            div()
                                .id(("field-box", index))
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when_some(brand, |box_, brand| {
                                    box_.child(
                                        img(format!("brands/{brand}.svg"))
                                            .id(("field-brand", index))
                                            .size_4()
                                            .flex_shrink_0(),
                                    )
                                })
                                .when_some(tip, |box_, tip| {
                                    box_.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                    })
                                })
                                .child(
                                    Checkbox::new(("field", index))
                                        .small()
                                        .label(label)
                                        .checked(*checked)
                                        .disabled(self.pending || self.model["fields"][index]["disabled"] == true)
                                        .on_click(cx.listener(move |form, checked, _, cx| {
                                            form.fields[field_index] = Field::Bool(*checked);
                                            cx.notify();
                                        })),
                                ),
                        );
                        index += 1;
                    }
                    body = body.child(row);
                    wanted += 30.;
                    continue;
                }
                if self.model["fields"][index]["hidden"] == true {
                    index += 1;
                    continue;
                }
                let label = self.model["fields"][index]["label"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let (editor, height) = match &self.fields[index] {
                    Field::Choice { select, input, .. } => {
                        let mut row = div().flex().gap_2().items_center();
                        if let Some(input) = input {
                            row = row.child(
                                div().flex_1().min_w_0().child(
                                    Input::new(input)
                                        .small()
                                        .aria_label(label.clone())
                                        .disabled(self.pending),
                                ),
                            );
                        }
                        let picker = Select::new(select).small().disabled(self.pending);
                        row = row.child(if input.is_some() {
                            div().w(px(150.)).flex_shrink_0().child(picker)
                        } else {
                            div().flex_1().min_w_0().child(picker)
                        });
                        (row.into_any_element(), 26.)
                    }
                    Field::Text(input) => {
                        let input = Input::new(input)
                            .small()
                            .appearance(false)
                            .aria_label(label.clone())
                            .disabled(self.pending);
                        let input = if self.model["fields"][index]["widget"]["kind"] == "secret" {
                            input.mask_toggle()
                        } else {
                            input
                        };
                        (boxed(input, 26., cx), 26.)
                    }
                    Field::Multi(input) => (
                        boxed(
                            Textarea::new(input)
                                .appearance(false)
                                .aria_label(label.clone())
                                .h(px(78.))
                                .disabled(self.pending),
                            80.,
                            cx,
                        ),
                        80.,
                    ),
                    Field::Pick(input) => (
                        Select::new(input).small().disabled(self.pending).into_any_element(),
                        26.,
                    ),
                    Field::Bool(_) => unreachable!(),
                };
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(height))
                        .flex_shrink_0()
                        .child(div().w(px(150.)).flex_shrink_0().truncate().child(label))
                        .child(div().flex_1().min_w_0().child(editor)),
                );
                wanted += height + 4.;
                index += 1;
            }
        }
        let height = (wanted + 42.).min((f32::from(window.viewport_size().height) - 170.).clamp(160., 600.));
        let mut footer = div().flex().items_center().gap_2();
        if self.json_editor.is_none() && self.model["json"] != false {
            footer = footer.child(
                Button::new("edit-json")
                    .icon(gpui_kit::component::Icon::empty().path("icons/pencil.svg"))
                    .label(tr("btn.edit_json"))
                    .disabled(self.pending)
                    .on_click(cx.listener(|form, _, _, cx| form.submit(true, cx))),
            );
        }
        footer = footer
            .child(div().flex_1())
            .child(
                Button::new("save-form")
                    .label(
                        self.model["okLabel"]
                            .as_str()
                            .map_or_else(|| tr("dialogs.ok").to_owned(), str::to_owned),
                    )
                    .disabled(self.pending)
                    .on_click(cx.listener(|form, _, _, cx| form.submit(false, cx))),
            )
            .child(
                Button::new("cancel-form")
                    .label(tr("dialogs.cancel"))
                    .disabled(self.pending)
                    .on_click(cx.listener(|_, _, window, cx| {
                        cx.emit(Cancelled);
                        window.close_dialog(cx);
                    })),
            );
        div()
            .id("editor-form")
            .test_support()
            .role(Role::Group)
            .aria_label(self.model["title"].as_str().unwrap_or_default().to_owned())
            .text_size(px(14.))
            .flex()
            .flex_col()
            .gap_3()
            .h(px(height))
            .child(body)
            .child(footer)
    }
}

/// 1.6.0 draws a text field as a dark box with a quiet border.
fn boxed(field: impl IntoElement, height: f32, cx: &App) -> AnyElement {
    use gpui_kit::component::ActiveTheme;
    div()
        .h(px(height))
        .bg(cx.theme().table)
        .border_1()
        .border_color(cx.theme().input)
        .rounded(px(4.))
        .child(field)
        .into_any_element()
}
