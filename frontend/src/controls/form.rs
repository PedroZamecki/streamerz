//! Shared schema-driven forms. Validation and type conversion stay in the core.
use super::gallery::Gallery;
use crate::{i18n::tr, send};
use gpui_kit::component::{
    button::Button,
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    select::{SearchableVec, Select, SelectEvent, SelectState},
    slider::{Slider, SliderEvent, SliderState},
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
        /// The list ends with "Other": the box to type a new one shows only when it is picked.
        other: bool,
    },
    /// Lines of text with a list to add from: picking a name adds it as a line.
    Lines {
        area: Entity<TextareaState>,
        select: Entity<SelectState<Vec<String>>>,
    },
    /// A field picked by its picture (TikTok's gifts): see `gallery.rs`.
    Gallery(Entity<Gallery>),
    /// A colour, picked from a palette or mixed with sliders: `#rrggbb`, or `#rrggbbaa` when see-through.
    Color(Entity<ColorPickerState>),
    /// A number dragged between two ends.
    Slider(Entity<SliderState>),
    /// A font of this computer, found by typing: the list starts with the system's (an empty name).
    Font {
        select: Entity<SelectState<SearchableVec<String>>>,
        /// What the first line of the list says (the system's font): it stands for no name.
        system: String,
    },
    /// A few boxes to tick on one line (the platforms): its value is the list of the ticked ones' ids.
    Checks {
        ids: Vec<String>,
        on: Vec<bool>,
    },
}

/// A colour as the core keeps it: `#rrggbb`, with `aa` only when it is see-through.
fn hex(color: Hsla) -> String {
    let c = Rgba::from(color);
    let byte = |part: f32| (part.clamp(0., 1.) * 255.).round() as u8;
    let (r, g, b, a) = (byte(c.r), byte(c.g), byte(c.b), byte(c.a));
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// `#rrggbb` or `#rrggbbaa` as a colour.
fn color_of(text: &str) -> Option<Hsla> {
    let hex = text.trim().strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
        return None;
    }
    let part = |at: usize| {
        u8::from_str_radix(&hex[at..at + 2], 16)
            .ok()
            .map(|v| f32::from(v) / 255.)
    };
    let a = if hex.len() == 8 { part(6)? } else { 1. };
    Some(Hsla::from(Rgba {
        r: part(0)?,
        g: part(2)?,
        b: part(4)?,
        a,
    }))
}

/// A slider's number without the noise of its 32 bits (0.85, not 0.8500000238).
fn tidy(value: f32) -> f64 {
    (f64::from(value) * 1e4).round() / 1e4
}
pub struct Cancelled;
pub struct Form {
    pub model: Value,
    fields: Vec<Field>,
    route: String,
    json_editor: Option<Entity<TextareaState>>,
    pub error: String,
    pub pending: bool,
    buttons_view: Entity<FormButtons>,
    /// A file picked with Browse, for its field (set into the box on the next draw, which has the window).
    picked: Option<(usize, String)>,
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
                if field["gallery"].is_object() {
                    let route = route.replace(":save", &format!(":gallery:{index}"));
                    let gallery = cx.new(|cx| Gallery::new(field, route, window, cx));
                    subscriptions.push(cx.observe(&gallery, |_, _, cx| cx.notify()));
                    return Field::Gallery(gallery);
                }
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
                    let other = !strict && field["other"] == true;
                    if other {
                        ids.push(String::new());
                        names.push(tr("forms.other_type").to_owned());
                    }
                    // A stored id the list no longer has stays selectable, as itself.
                    if strict && !ids.iter().any(|id| id == current) {
                        ids.push(current.to_owned());
                        names.push(current.to_owned());
                    }
                    let selected = ids
                        .iter()
                        .position(|id| id == current && !(other && id.is_empty()))
                        .or((other && !current.is_empty() && !ids.iter().any(|id| id == current))
                            .then(|| ids.len() - 1))
                        .map(IndexPath::new);
                    // A list that also takes typed text (TikTok's gifts, OBS scenes) or a long one can be searched.
                    let searchable = !strict || names.len() > 8;
                    let select = cx.new(|cx| SelectState::new(names, selected, window, cx).searchable(searchable));
                    let hint = field["placeholder"].as_str().unwrap_or_default().to_owned();
                    let input = (!strict).then(|| {
                        cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(current.to_owned())
                                .placeholder(hint)
                        })
                    });
                    if let Some(input) = input.clone() {
                        // A typed id picks its name in the list (or none when the list doesn't have it).
                        let (chosen, list) = (ids.clone(), select.clone());
                        subscriptions.push(cx.subscribe_in(
                            &input,
                            window,
                            move |_, input, event: &InputEvent, window, cx| {
                                if !matches!(event, InputEvent::Change) {
                                    return;
                                }
                                let typed = input.read(cx).value().trim().to_owned();
                                let at = chosen.iter().position(|id| *id == typed).map(IndexPath::new);
                                if list.read(cx).selected_index(cx) != at {
                                    list.update(cx, |list, cx| list.set_selected_index(at, window, cx));
                                }
                            },
                        ));
                    }
                    if let Some(input) = input.clone() {
                        let chosen = ids.clone();
                        subscriptions.push(cx.subscribe_in(
                            &select,
                            window,
                            move |_, select, event: &SelectEvent<Vec<String>>, window, cx| {
                                if let SelectEvent::Confirm(Some(_)) = event {
                                    let id = select
                                        .read(cx)
                                        .selected_index(cx)
                                        .and_then(|at| chosen.get(at.row).cloned());
                                    if let Some(id) = id {
                                        input.update(cx, |input, cx| input.set_value(id, window, cx));
                                    }
                                }
                            },
                        ));
                    }
                    let _ = index;
                    return Field::Choice {
                        select,
                        input,
                        ids,
                        other,
                    };
                }
                match kind {
                    "font" => {
                        let mut names = cx.text_system().all_font_names();
                        names.sort_by_key(|name| name.to_lowercase());
                        names.dedup();
                        // A dot at the start marks a font the system keeps for itself.
                        names.retain(|name| !name.starts_with('.'));
                        let chosen = value.as_str().unwrap_or_default();
                        // One typed by hand before (in the JSON) that is not installed stays in the list.
                        if !chosen.is_empty() && !names.iter().any(|name| name == chosen) {
                            names.insert(0, chosen.to_owned());
                        }
                        let system = tr("forms.font.system").to_owned();
                        let at = names.iter().position(|name| name == chosen).map_or(0, |at| at + 1);
                        names.insert(0, system.clone());
                        // A list that narrows as its name is typed: there are hundreds of fonts.
                        let select = cx.new(|cx| {
                            SelectState::new(SearchableVec::new(names), Some(IndexPath::new(at)), window, cx)
                                .searchable(true)
                        });
                        subscriptions.push(
                            cx.subscribe(&select, |_, _, _: &SelectEvent<SearchableVec<String>>, cx| cx.notify()),
                        );
                        Field::Font { select, system }
                    }
                    "color" => {
                        let color = value.as_str().and_then(color_of);
                        let state = cx.new(|cx| {
                            let state = ColorPickerState::new(window, cx);
                            match color {
                                Some(color) => state.default_value(color),
                                None => state,
                            }
                        });
                        subscriptions.push(cx.subscribe(&state, |_, _, _: &ColorPickerEvent, cx| cx.notify()));
                        Field::Color(state)
                    }
                    "slider" => {
                        let end = |key: &str, or: f64| field["widget"][key].as_f64().unwrap_or(or) as f32;
                        let state = cx.new(|_| {
                            SliderState::new()
                                .min(end("min", 0.))
                                .max(end("max", 100.))
                                .step(end("step", 1.))
                                .default_value(value.as_f64().unwrap_or(0.) as f32)
                        });
                        subscriptions.push(cx.subscribe(&state, |_, _, _: &SliderEvent, cx| cx.notify()));
                        Field::Slider(state)
                    }
                    "bool" | "bit" => Field::Bool(value.as_bool().unwrap_or(false)),
                    "checks" => {
                        let ids: Vec<String> = field["widget"]["options"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|option| option["id"].as_str().map(str::to_owned))
                            .collect();
                        let ticked: Vec<&str> = value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        let on = ids.iter().map(|id| ticked.contains(&id.as_str())).collect();
                        Field::Checks { ids, on }
                    }
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
                        let state = cx.new(|cx| SelectState::new(options, selected, window, cx));
                        // Another field may show or hide by what is picked here.
                        subscriptions.push(cx.subscribe(&state, |_, _, _: &SelectEvent<Vec<String>>, cx| cx.notify()));
                        Field::Pick(state)
                    }
                    "multi" | "lines" | "map" => {
                        let hint = field["placeholder"].as_str().unwrap_or_default().to_owned();
                        let area = cx.new(|cx| {
                            TextareaState::new(window, cx)
                                .default_value(value.as_str().unwrap_or_default().to_owned())
                                .placeholder(hint)
                        });
                        let names: Vec<String> = field["suggest"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|pick| pick["label"].as_str().map(str::to_owned))
                            .collect();
                        if names.is_empty() {
                            return Field::Multi(area);
                        }
                        let select = cx.new(|cx| SelectState::new(names.clone(), None, window, cx).searchable(true));
                        let (target, list) = (area.clone(), names.clone());
                        subscriptions.push(cx.subscribe_in(
                            &select,
                            window,
                            move |_, select, event: &SelectEvent<Vec<String>>, window, cx| {
                                let SelectEvent::Confirm(Some(_)) = event else { return };
                                let Some(name) = select
                                    .read(cx)
                                    .selected_index(cx)
                                    .and_then(|at| list.get(at.row).cloned())
                                else {
                                    return;
                                };
                                let text = target.read(cx).value().to_string();
                                if !text.lines().any(|line| line.trim().eq_ignore_ascii_case(&name)) {
                                    let joined = if text.trim().is_empty() {
                                        name
                                    } else {
                                        format!("{}\n{name}", text.trim_end())
                                    };
                                    target.update(cx, |area, cx| area.set_value(joined, window, cx));
                                }
                                select.update(cx, |select, cx| select.set_selected_index(None, window, cx));
                            },
                        ));
                        Field::Lines { area, select }
                    }
                    _ => {
                        // A hint in the empty box: an example, or where the value goes.
                        let hint = field["placeholder"].as_str().unwrap_or_default().to_owned();
                        let input = cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(value.as_str().unwrap_or_default().to_owned())
                                .masked(kind == "secret")
                                .placeholder(hint)
                        });
                        // A picked image shows as it's typed.
                        if field["browse"] == "image" {
                            subscriptions.push(cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                                if matches!(event, InputEvent::Change) {
                                    cx.notify();
                                }
                            }));
                        }
                        Field::Text(input)
                    }
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
            buttons_view: {
                let weak = cx.entity().downgrade();
                cx.new(|_| FormButtons(weak))
            },
            picked: None,
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
    /// What a field holds now, as the core takes it (none: a list with nothing picked).
    fn value(&self, index: usize, cx: &App) -> Option<Value> {
        Some(match self.fields.get(index)? {
            Field::Bool(value) => json!(value),
            Field::Text(input) => json!(input.read(cx).value().to_string()),
            Field::Multi(input) | Field::Lines { area: input, .. } => json!(input.read(cx).value().to_string()),
            Field::Pick(input) => json!(input.read(cx).selected_index(cx)?.row),
            Field::Choice { input: Some(input), .. } => json!(input.read(cx).value().to_string()),
            Field::Gallery(gallery) => json!(gallery.read(cx).selected),
            Field::Color(state) => json!(hex(state.read(cx).value()?)),
            Field::Slider(state) => json!(tidy(state.read(cx).value().start())),
            Field::Checks { ids, on } => json!(ids
                .iter()
                .zip(on)
                .filter(|(_, on)| **on)
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()),
            Field::Font { select, system } => match select.read(cx).selected_value() {
                Some(name) if name != system => json!(name),
                _ => json!(""),
            },
            Field::Choice { select, ids, .. } => {
                json!(ids
                    .get(select.read(cx).selected_index(cx)?.row)
                    .cloned()
                    .unwrap_or_default())
            }
        })
    }

    /// Whether a field is drawn: not hidden by the core, and the field it depends on (`showIf`) has
    /// the value that calls for it.
    fn shown(&self, index: usize, cx: &App) -> bool {
        let field = &self.model["fields"][index];
        if field["hidden"] == true {
            return false;
        }
        let when = &field["showIf"];
        match when["field"].as_u64() {
            Some(other) => self
                .value(other as usize, cx)
                .is_some_and(|value| value == when["equals"]),
            None => true,
        }
    }

    fn changes(&self, cx: &App) -> Vec<Value> {
        (0..self.fields.len())
            .filter_map(|index| {
                let value = self.value(index, cx)?;
                (value != self.model["fields"][index]["value"]).then(|| json!({"field": index, "value": value}))
            })
            .collect()
    }
}
impl Form {
    /// A reply for field `index`'s picture picker: its pictures (`images`) or its reloaded list (`choices`).
    pub fn gallery_reply(&mut self, index: usize, what: &str, reply: &Value, cx: &mut Context<Self>) {
        if let Some(Field::Gallery(gallery)) = self.fields.get(index) {
            gallery.update(cx, |gallery, cx| match what {
                "images" => gallery.images(reply, cx),
                _ => gallery.set_choices(reply, cx),
            });
        }
    }

    /// The core downloaded pictures: the picture pickers ask for theirs again.
    pub fn images_arrived(&mut self, cx: &mut Context<Self>) {
        for field in &self.fields {
            if let Field::Gallery(gallery) = field {
                gallery.update(cx, |gallery, _| gallery.images_arrived());
            }
        }
    }

    /// Opens the system's file dialog for field `index`; the file picked goes in its box.
    fn browse(&mut self, index: usize, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |form, cx| {
            let picked = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Err(error)) => {
                    let _ = form.update(cx, |form, cx| {
                        form.error = error.to_string();
                        cx.notify();
                    });
                    None
                }
                _ => None,
            };
            if let Some(path) = picked {
                let _ = form.update(cx, |form, cx| {
                    form.picked = Some((index, path.display().to_string()));
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
/// How tall a quiet line of text is drawn, wrapped at the dialog's width (about 85 characters a line).
fn wrapped(text: &str, line: f32) -> f32 {
    line * (text.chars().count() as f32 / 85.).ceil().max(1.)
}
impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some((index, path)) = self.picked.take() {
            match self.fields.get(index) {
                Some(Field::Text(input)) => input.update(cx, |input, cx| input.set_value(path, window, cx)),
                // A list that also takes typed text: the file picked is typed, under "Other".
                Some(Field::Choice {
                    select,
                    input: Some(input),
                    ids,
                    ..
                }) => {
                    let last = ids.len().checked_sub(1).map(IndexPath::new);
                    select.update(cx, |select, cx| select.set_selected_index(last, window, cx));
                    input.update(cx, |input, cx| input.set_value(path, window, cx));
                }
                _ => {}
            }
        }
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
            wanted += wrapped(hint, 18.) + 8.;
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
            body = body.child(super::notice::info(note, cx).flex_shrink_0());
            wanted += 18. * (note.chars().count() as f32 / 85.).ceil().max(1.) + 4.;
        }
        if let Some(editor) = &self.json_editor {
            wanted += 440.;
            body = body.child(Textarea::new(editor).aria_label(tr("btn.edit_json")).h_full());
        } else {
            let mut index = 0;
            while index < self.fields.len() {
                // A heading above the field that starts a part of the form.
                if let Some(title) = self.model["fields"][index]["section"].as_str() {
                    body = body.child(
                        div()
                            .flex_shrink_0()
                            .mt(px(if index == 0 { 0. } else { 6. }))
                            .pb(px(2.))
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .text_size(px(12.))
                            .text_color(crate::theme::quiet(cx))
                            .child(title.to_owned()),
                    );
                    wanted += 30.;
                }
                if !self.shown(index, cx) {
                    index += 1;
                    continue;
                }
                if matches!(self.fields[index], Field::Bool(_)) {
                    let mut row = div().flex().items_center().gap_2().h(px(26.)).flex_shrink_0();
                    for place in 0..2 {
                        let Some(Field::Bool(checked)) = self.fields.get(index) else {
                            break;
                        };
                        // The second box of a line may be one that is not shown now: it is skipped next.
                        if place > 0 && !self.shown(index, cx) {
                            break;
                        }
                        // A box that starts a part of the form goes on a line of its own, under the heading.
                        if place > 0 && self.model["fields"][index]["section"].is_string() {
                            break;
                        }
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
                let label = self.model["fields"][index]["label"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let (editor, height) = match &self.fields[index] {
                    Field::Gallery(gallery) => (Gallery::field(gallery, label.clone(), self.pending, cx), 26.),
                    Field::Choice {
                        select,
                        input,
                        ids,
                        other,
                    } => {
                        let mut row = div().flex().gap_2().items_center();
                        let typing = !*other
                            || select
                                .read(cx)
                                .selected_index(cx)
                                .is_some_and(|at| at.row + 1 == ids.len());
                        // With "Other" in the list, the box shows only once it is picked.
                        if let Some(input) = input.as_ref().filter(|_| typing || ids.len() <= 1) {
                            row = row.child(
                                div().flex_1().min_w_0().child(boxed(
                                    Input::new(input)
                                        .small()
                                        .appearance(false)
                                        .aria_label(label.clone())
                                        .disabled(self.pending),
                                    26.,
                                    cx,
                                )),
                            );
                        }
                        // A field that also takes typed text shows the list only when something was found.
                        if (input.is_none() || !ids.is_empty()) && !(*other && ids.len() <= 1) {
                            let picker = Select::new(select)
                                .small()
                                .placeholder(tr("forms.pick_from_list"))
                                .search_placeholder(tr("lists.search"))
                                .disabled(self.pending);
                            row = row.child(div().flex_1().min_w_0().child(picker));
                        }
                        // A file of the overlay, or one to bring in from this computer.
                        if self.model["fields"][index]["browse"].is_string() {
                            let field_index = index;
                            row = row.child(
                                Button::new(("browse", index))
                                    .small()
                                    .h(px(26.))
                                    .icon(gpui_kit::component::Icon::empty().path("icons/folder-open.svg"))
                                    .label(tr("btn.browse"))
                                    .disabled(self.pending)
                                    .on_click(cx.listener(move |form, _, _, cx| form.browse(field_index, cx))),
                            );
                        }
                        (row.into_any_element(), 26.)
                    }
                    Field::Checks { ids, on } => {
                        let options = &self.model["fields"][index]["widget"]["options"];
                        let field_index = index;
                        // One line: each box with its logo, as close as they read well.
                        let mut row = div().flex().items_center().gap(px(10.)).overflow_hidden();
                        for (at, _) in ids.iter().enumerate() {
                            let name = options[at]["label"].as_str().unwrap_or_default().to_owned();
                            let brand = options[at]["brand"].as_str().map(str::to_owned);
                            row = row.child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(px(3.))
                                    .when_some(brand, |box_, brand| {
                                        box_.child(
                                            img(format!("brands/{brand}.svg"))
                                                .id(("check-brand", index * 16 + at))
                                                .size_4()
                                                .flex_shrink_0(),
                                        )
                                    })
                                    .child(
                                        Checkbox::new(("check", index * 16 + at))
                                            .small()
                                            .label(name)
                                            .checked(on[at])
                                            .disabled(self.pending)
                                            .on_click(cx.listener(move |form, checked: &bool, _, cx| {
                                                if let Some(Field::Checks { on, .. }) = form.fields.get_mut(field_index)
                                                {
                                                    on[at] = *checked;
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            );
                        }
                        (row.into_any_element(), 26.)
                    }
                    Field::Text(state) => {
                        let input = Input::new(state)
                            .small()
                            .appearance(false)
                            .aria_label(label.clone())
                            .disabled(self.pending);
                        let input = if self.model["fields"][index]["widget"]["kind"] == "secret" {
                            input.mask_toggle()
                        } else {
                            input
                        };
                        let browse = self.model["fields"][index]["browse"].as_str().unwrap_or_default();
                        if browse.is_empty() {
                            (boxed(input, 26., cx), 26.)
                        } else {
                            // A file's box with Browse beside it; an image shows under it once it's found.
                            let field_index = index;
                            let line = div()
                                .flex()
                                .gap_2()
                                .child(div().flex_1().min_w_0().child(boxed(input, 26., cx)))
                                .child(
                                    Button::new(("browse", index))
                                        .small()
                                        .h(px(26.))
                                        .icon(gpui_kit::component::Icon::empty().path("icons/folder-open.svg"))
                                        .label(tr("btn.browse"))
                                        .disabled(self.pending)
                                        .on_click(cx.listener(move |form, _, _, cx| form.browse(field_index, cx))),
                                );
                            let file = state.read(cx).value().trim().trim_matches('"').to_owned();
                            let image = browse == "image" && !file.is_empty() && std::path::Path::new(&file).is_file();
                            if image {
                                let preview = img(std::path::PathBuf::from(file))
                                    .id(("browse-preview", index))
                                    .h(px(126.))
                                    .w(px(224.))
                                    .rounded(px(4.))
                                    .object_fit(ObjectFit::Contain);
                                (
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(line)
                                        .child(preview)
                                        .into_any_element(),
                                    160.,
                                )
                            } else {
                                (line.into_any_element(), 26.)
                            }
                        }
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
                    Field::Lines { area, select, .. } => {
                        let picker = Select::new(select)
                            .small()
                            .placeholder(tr("forms.pick_from_list"))
                            .search_placeholder(tr("lists.search"))
                            .disabled(self.pending);
                        let area = boxed(
                            Textarea::new(area)
                                .appearance(false)
                                .aria_label(label.clone())
                                .h(px(78.))
                                .disabled(self.pending),
                            80.,
                            cx,
                        );
                        (
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(picker)
                                .child(area)
                                .into_any_element(),
                            114.,
                        )
                    }
                    Field::Pick(input) => (
                        Select::new(input)
                            .small()
                            .when_some(
                                self.model["fields"][index]["placeholder"].as_str().map(str::to_owned),
                                |select, hint| select.placeholder(hint),
                            )
                            .disabled(self.pending)
                            .into_any_element(),
                        26.,
                    ),
                    Field::Font { select, .. } => (
                        Select::new(select).small().disabled(self.pending).into_any_element(),
                        26.,
                    ),
                    Field::Color(state) => {
                        let shown = state.read(cx).value().map(hex).unwrap_or_default();
                        (
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(ColorPicker::new(state).small())
                                .child(div().text_color(crate::theme::quiet(cx)).child(shown))
                                .into_any_element(),
                            26.,
                        )
                    }
                    Field::Slider(state) => {
                        let shown = tidy(state.read(cx).value().start()).to_string();
                        (
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(Slider::new(state).disabled(self.pending)),
                                )
                                .child(
                                    div()
                                        .w(px(52.))
                                        .flex_shrink_0()
                                        .text_color(crate::theme::quiet(cx))
                                        .child(shown),
                                )
                                .into_any_element(),
                            26.,
                        )
                    }
                    Field::Bool(_) => unreachable!(),
                };
                // What the field is for, when its maker said: shown over its label.
                let about = self.model["fields"][index]["tip"].as_str().map(str::to_owned);
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(height))
                        .flex_shrink_0()
                        .child({
                            let label_tip = about.unwrap_or_else(|| label.clone());
                            div()
                                .id(("form-lbl", index))
                                .w(px(150.))
                                .flex_shrink_0()
                                .truncate()
                                .child(label)
                                .when(!label_tip.is_empty(), |el| {
                                    el.tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(label_tip.clone()).build(window, cx)
                                    })
                                })
                        })
                        .child(div().flex_1().min_w_0().child(editor)),
                );
                wanted += height + 4.;
                index += 1;
            }
        }
        let height = wanted.min((f32::from(window.viewport_size().height) - 220.).clamp(120., 560.));
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
    }
}

impl Form {
    /// The form's buttons, for the dialog's footer: Edit JSON at the left, OK and Cancel at the right.
    pub fn buttons(&self, cx: &mut Context<Self>) -> Div {
        let left = (self.json_editor.is_none() && self.model["json"] != false).then(|| {
            Button::new("edit-json")
                .icon(gpui_kit::component::Icon::empty().path("icons/pencil.svg"))
                .label(tr("btn.edit_json"))
                .disabled(self.pending)
                .on_click(cx.listener(|form, _, _, cx| form.submit(true, cx)))
                .into_any_element()
        });
        let right = div()
            .flex()
            .gap_2()
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
        super::dialog::footer(left, right, cx)
    }

    /// The buttons as their own view, for a dialog's footer slot.
    pub fn buttons_view(&self) -> Entity<FormButtons> {
        self.buttons_view.clone()
    }
}

/// A form's buttons, drawn where its dialog puts them (the footer).
pub struct FormButtons(WeakEntity<Form>);
impl Render for FormButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.0.upgrade() {
            Some(form) => form.update(cx, |form, cx| form.buttons(cx)).into_any_element(),
            None => div().into_any_element(),
        }
    }
}

/// A text field is a dark box with a quiet border.
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
